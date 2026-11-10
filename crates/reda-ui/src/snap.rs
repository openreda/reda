//! Cursor snapping: one model for every editor (docs/spec/v0.1/editing.md#cursor-snapping).
//!
//! A snap is a pure function of the cursor, the [`Target`]s the editor gathered from its
//! Document ([`hotspots`]), any alignment [`Guide`]s, and the [`Rules`] in force. The
//! levels rank **Hotspot, then alignment guide, then grid**. Capture is a length checked
//! on both axes: the cursor must be within it in X and in Y. Boards and Footprints use one
//! Snap Distance (a Preference); sheets and Symbols use the grid preset's Electrical
//! Range ([`crate::shell::Shell::electrical_range`]).
//!
//! [`Rules`] come from the Document's working state (object snap mode, Gridless, the
//! Hotspot checklist), the Preferences and the keys held in flight ([`Suspend`]):
//! [`rules`] puts them together for an open Document.

pub mod hotspots;
mod palette;
mod session;
mod suspend;

use std::collections::BTreeSet;

use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};

pub use palette::{
    PaletteRequests, controls as palette_controls, mode_label, palette_button,
    register as register_palette,
};
pub use reda_engine::preferences::Hotspot;
pub use session::{held, rules, rules_on, snap_at, suspend, to_grid};
pub use suspend::Suspend;

use crate::bars::DocumentType;
use crate::canvas::interact::closest_on_segment;

/// The Hotspot checklist a Document type offers, in the order the palette lists it.
pub fn offered(ty: DocumentType) -> &'static [Hotspot] {
    use Hotspot::*;
    match ty {
        DocumentType::Pcb | DocumentType::Footprint => &[
            PadCenter,
            ViaCenter,
            TrackEnd,
            ArcEnd,
            ArcCenter,
            OutlineVertex,
            FootprintOrigin,
            TextAnchor,
        ],
        DocumentType::Schematic | DocumentType::Symbol => {
            &[PinEnd, WireEnd, WireVertex, Wire, PortAnchor, LabelAnchor]
        }
    }
}

/// The kind's name, as Commands and `.reda/view.json` spell it.
pub fn hotspot_name(kind: Hotspot) -> &'static str {
    use Hotspot::*;
    match kind {
        PadCenter => "pad_center",
        ViaCenter => "via_center",
        TrackEnd => "track_end",
        ArcEnd => "arc_end",
        ArcCenter => "arc_center",
        OutlineVertex => "outline_vertex",
        FootprintOrigin => "footprint_origin",
        TextAnchor => "text_anchor",
        PinEnd => "pin_end",
        WireEnd => "wire_end",
        WireVertex => "wire_vertex",
        Wire => "wire",
        PortAnchor => "port_anchor",
        LabelAnchor => "label_anchor",
    }
}

/// The checklist's words for `kind`, such as "Pad centre".
pub fn hotspot_label(kind: Hotspot) -> String {
    crate::i18n::translate(
        &format!("snap-hotspot-{}", hotspot_name(kind).replace('_', "-")),
        &[],
    )
}

/// Where a Target lies, for the Active Layer mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnLayers {
    /// Not on a Layer (a Footprint origin, the Board Outline): snaps in every mode but Off.
    Every,
    /// On every copper Layer: through-hole Pads and Vias.
    Copper,
    /// On one Layer.
    One(Uuid),
}

/// Where on a Target the cursor lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Point(Point),
    /// Anywhere along it: on the grid where it runs straight across the grid.
    Segment(Point, Point),
}

/// One thing the cursor can snap to, and the object it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub shape: Shape,
    pub kind: Hotspot,
    pub object: Uuid,
    pub layers: OnLayers,
}

impl Target {
    pub fn point(at: Point, kind: Hotspot, object: Uuid, layers: OnLayers) -> Target {
        Target {
            shape: Shape::Point(at),
            kind,
            object,
            layers,
        }
    }

    /// The point Target's point, or a segment's first end.
    pub fn at(&self) -> Point {
        match self.shape {
            Shape::Point(p) | Shape::Segment(p, _) => p,
        }
    }
}

/// An alignment guide: a line the cursor snaps across to on one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guide {
    /// The vertical line at this x.
    X(Nm),
    /// The horizontal line at this y.
    Y(Nm),
}

/// The snap grid: its pitch and the point it runs through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub pitch: Nm,
    pub origin: Point,
}

impl Grid {
    /// The grid point nearest `at`.
    pub fn nearest(self, at: (f64, f64)) -> Point {
        let g = self.pitch.0.max(1) as f64;
        let (ox, oy) = (self.origin.x.0 as f64, self.origin.y.0 as f64);
        Point::new(
            Nm((((at.0 - ox) / g).round() * g + ox) as i64),
            Nm((((at.1 - oy) / g).round() * g + oy) as i64),
        )
    }
}

/// Which objects the cursor snaps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Objects {
    /// None: object snap is Off, or suspended.
    Off,
    /// Every object, whatever its Layer.
    All,
    /// Objects on this Layer, and when it is copper those on every copper Layer.
    Layer { layer: Uuid, copper: bool },
}

/// Everything that decides a snap besides the cursor and the Targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    /// How near, on both axes, a Hotspot or guide catches the cursor.
    pub capture: Nm,
    /// The grid level; `None` when Gridless, or suspended with `Mod`+Shift.
    pub grid: Option<Grid>,
    pub objects: Objects,
    /// Hotspot kinds turned off in the checklist.
    pub off: BTreeSet<Hotspot>,
    /// The alignment guide level is in force (`Mod` suspends it with object snap).
    pub guides: bool,
}

impl Rules {
    /// Only the grid (or free when there is none).
    pub fn grid_only(grid: Option<Grid>) -> Rules {
        Rules {
            capture: Nm(0),
            grid,
            objects: Objects::Off,
            off: BTreeSet::new(),
            guides: false,
        }
    }

    /// `at` on the grid level alone: the nearest grid point, or `at` rounded with none.
    pub fn to_grid(&self, at: (f64, f64)) -> Point {
        match self.grid {
            Some(grid) => grid.nearest(at),
            None => round(at),
        }
    }

    fn takes(&self, target: &Target) -> bool {
        if self.off.contains(&target.kind) {
            return false;
        }
        match (self.objects, target.layers) {
            (Objects::Off, _) => false,
            (Objects::All, _) | (_, OnLayers::Every) => true,
            (Objects::Layer { copper, .. }, OnLayers::Copper) => copper,
            (Objects::Layer { layer, .. }, OnLayers::One(on)) => on == layer,
        }
    }

    fn captures(&self, at: (f64, f64), p: (f64, f64)) -> bool {
        let r = self.capture.0 as f64;
        (p.0 - at.0).abs() <= r && (p.1 - at.1).abs() <= r
    }
}

/// What a snapped point hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A Hotspot of `object`.
    Hotspot { kind: Hotspot, object: Uuid },
    /// An alignment guide, on one axis or both.
    Guide,
    /// The grid.
    Grid,
    /// Nothing: Gridless (or the grid suspended) and no Hotspot near.
    Free,
}

impl Hit {
    /// The Hotspot kind hit, if any.
    pub fn hotspot(self) -> Option<Hotspot> {
        match self {
            Hit::Hotspot { kind, .. } => Some(kind),
            _ => None,
        }
    }

    /// The marker's words: what the point hit, such as "Pad centre".
    pub fn label(self) -> String {
        match self {
            Hit::Hotspot { kind, .. } => hotspot_label(kind),
            Hit::Guide => crate::tr!("snap-hit-guide"),
            Hit::Grid => crate::tr!("snap-hit-grid"),
            Hit::Free => crate::tr!("snap-hit-free"),
        }
    }
}

/// Paints the snap marker at the screen point `c`: a cross on the grid, a diamond on an
/// alignment guide, and a ring naming the Hotspot it hit. Free points have none.
pub fn marker(painter: &egui::Painter, c: egui::Pos2, hit: Hit, color: egui::Color32) {
    use egui::{Stroke, vec2};
    match hit {
        Hit::Grid => {
            let r = 4.0;
            painter.line_segment(
                [c - vec2(r, 0.0), c + vec2(r, 0.0)],
                Stroke::new(1.0, color),
            );
            painter.line_segment(
                [c - vec2(0.0, r), c + vec2(0.0, r)],
                Stroke::new(1.0, color),
            );
        }
        Hit::Free => {}
        Hit::Guide => {
            let r = 5.0;
            let points = vec![
                c + vec2(0.0, -r),
                c + vec2(r, 0.0),
                c + vec2(0.0, r),
                c + vec2(-r, 0.0),
                c + vec2(0.0, -r),
            ];
            painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
        }
        Hit::Hotspot { .. } => {
            painter.circle_stroke(c, 6.0, Stroke::new(2.0, color));
            painter.text(
                c + vec2(9.0, -9.0),
                egui::Align2::LEFT_BOTTOM,
                hit.label(),
                egui::FontId::proportional(11.0),
                color,
            );
        }
    }
}

/// Where the cursor snapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapped {
    pub at: Point,
    pub hit: Hit,
}

/// `at` (nm) snapped by `rules`: onto the nearest captured point Hotspot, else the
/// nearest captured segment, else the alignment guides on each axis they catch (the other
/// axis on the grid), else the grid, else left where it is.
pub fn snap(at: (f64, f64), targets: &[Target], guides: &[Guide], rules: &Rules) -> Snapped {
    let dist = |p: (f64, f64)| (p.0 - at.0).hypot(p.1 - at.1);
    let usable = || targets.iter().filter(|t| rules.takes(t));
    let point = usable()
        .filter_map(|t| match t.shape {
            Shape::Point(p) => Some((p, t)),
            Shape::Segment(..) => None,
        })
        .filter(|(p, _)| rules.captures(at, f(*p)))
        .min_by(|a, b| dist(f(a.0)).total_cmp(&dist(f(b.0))));
    if let Some((p, t)) = point {
        return Snapped {
            at: p,
            hit: Hit::Hotspot {
                kind: t.kind,
                object: t.object,
            },
        };
    }
    let on_segment = usable()
        .filter_map(|t| match t.shape {
            Shape::Segment(a, b) => Some((closest_on_segment(at, f(a), f(b)), a, b, t)),
            Shape::Point(_) => None,
        })
        .filter(|(c, ..)| rules.captures(at, *c))
        .min_by(|x, y| dist(x.0).total_cmp(&dist(y.0)));
    if let Some((c, a, b, t)) = on_segment {
        let grid = rules.to_grid(c);
        let (lo, hi) = (
            Point::new(a.x.min(b.x), a.y.min(b.y)),
            Point::new(a.x.max(b.x), a.y.max(b.y)),
        );
        let p = if a.x == b.x {
            Point::new(a.x, grid.y.clamp(lo.y, hi.y))
        } else if a.y == b.y {
            Point::new(grid.x.clamp(lo.x, hi.x), a.y)
        } else {
            round(c)
        };
        return Snapped {
            at: p,
            hit: Hit::Hotspot {
                kind: t.kind,
                object: t.object,
            },
        };
    }
    let near = |values: &mut dyn Iterator<Item = Nm>, v: f64| {
        values
            .map(|g| (g, (g.0 as f64 - v).abs()))
            .filter(|(_, d)| *d <= rules.capture.0 as f64)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(g, _)| g)
    };
    let guides = if rules.guides { guides } else { &[] };
    let gx = near(
        &mut guides.iter().filter_map(|g| match g {
            Guide::X(x) => Some(*x),
            Guide::Y(_) => None,
        }),
        at.0,
    );
    let gy = near(
        &mut guides.iter().filter_map(|g| match g {
            Guide::Y(y) => Some(*y),
            Guide::X(_) => None,
        }),
        at.1,
    );
    let grid = rules.to_grid(at);
    if gx.is_some() || gy.is_some() {
        return Snapped {
            at: Point::new(gx.unwrap_or(grid.x), gy.unwrap_or(grid.y)),
            hit: Hit::Guide,
        };
    }
    Snapped {
        at: grid,
        hit: if rules.grid.is_some() {
            Hit::Grid
        } else {
            Hit::Free
        },
    }
}

/// The nearest point Target to `at` within `capture` on both axes, whatever its kind or
/// Layer: where a grabbed object is held.
pub fn nearest(targets: &[Target], at: Point, capture: Nm) -> Option<Target> {
    let r = capture.0;
    let d = |p: Point| ((p.x.0 - at.x.0) as f64).hypot((p.y.0 - at.y.0) as f64);
    targets
        .iter()
        .filter_map(|t| match t.shape {
            Shape::Point(p) => Some((p, t)),
            Shape::Segment(..) => None,
        })
        .filter(|(p, _)| (p.x.0 - at.x.0).abs() <= r && (p.y.0 - at.y.0).abs() <= r)
        .min_by(|a, b| d(a.0).total_cmp(&d(b.0)))
        .map(|(_, t)| *t)
}

fn f(p: Point) -> (f64, f64) {
    (p.x.0 as f64, p.y.0 as f64)
}

fn round(p: (f64, f64)) -> Point {
    Point::new(Nm(p.0.round() as i64), Nm(p.1.round() as i64))
}

#[cfg(test)]
mod tests;
