//! The graphics Tools on a sheet: Line draws a polyline, Rect a rectangle between two
//! corners; Text (a [`Placing`] Tool, see [`super::wiring`]) places free text.
//!
//! A shape in flight touches neither the Project nor the Journal. A polyline commits on
//! Enter (`tool.finish`) or a click on its last point again, as ONE Journal entry and
//! one undo step; a rectangle commits on its second corner (or at the end of a drag).
//! Backspace (`tool.undo_segment`) takes the last point back.

use reda_engine::model::geom::Point;
use serde_json::{Value, json};

use super::{Cx, P};
use crate::canvas::interact::Pointer;
use crate::tools::Tool;

/// The Line or Rect Tool: the points put down so far and where the pointer snapped.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    /// [`Tool::Line`] or [`Tool::Rect`].
    pub tool: Tool,
    /// The points put down: a polyline's corners, or a rectangle's first corner.
    pub points: Vec<Point>,
    /// Where the pointer snapped.
    pub at: Point,
    /// A finished shape's outline, still drawn until the Shell has run its Command.
    pub committed: Vec<(Point, Point)>,
}

impl Drawing {
    /// The shape as it would be drawn to the pointer.
    pub fn segments(&self) -> Vec<(Point, Point)> {
        match (self.tool, self.points.as_slice()) {
            (_, []) => Vec::new(),
            (Tool::Rect, [a, ..]) => outline(*a, self.at),
            _ => {
                let mut path = self.points.clone();
                if path.last() != Some(&self.at) {
                    path.push(self.at);
                }
                path.windows(2).map(|w| (w[0], w[1])).collect()
            }
        }
    }

    /// Backspace: takes the last point back.
    pub fn undo_point(&mut self) {
        self.points.pop();
    }
}

/// A rectangle's outline between opposite corners `a` and `b`.
fn outline(a: Point, b: Point) -> Vec<(Point, Point)> {
    let (c, d) = (Point::new(b.x, a.y), Point::new(a.x, b.y));
    vec![(a, c), (c, b), (b, d), (d, a)]
}

fn at(p: Point) -> Value {
    json!({"x": p.x.0, "y": p.y.0})
}

impl Cx<'_> {
    /// The state the Line or Rect Tool starts in with the pointer at `at`.
    pub(super) fn start_drawing(&self, tool: Tool, at: P) -> Drawing {
        Drawing {
            tool,
            points: Vec::new(),
            at: self.snap(at),
            committed: Vec::new(),
        }
    }

    /// The Line or Rect Tool on one pointer event.
    pub(super) fn draw_step(&mut self, mut d: Drawing, event: Pointer) -> Drawing {
        match event {
            Pointer::Hover(at) | Pointer::Drag(at) => d.at = self.snap(at),
            Pointer::Click { at, .. } | Pointer::DragEnd(at) => {
                d.at = self.snap(at);
                d = self.put_down(d);
            }
            Pointer::DragStart { from, at, .. } => {
                d.at = self.snap(from);
                d = self.put_down(d);
                d.at = self.snap(at);
            }
        }
        d
    }

    /// Puts a point down where the pointer snapped: a polyline ends on its last point
    /// again, a rectangle on its second corner.
    fn put_down(&mut self, mut d: Drawing) -> Drawing {
        let p = d.at;
        if d.points.is_empty() {
            d.committed.clear();
        }
        match d.tool {
            Tool::Rect if !d.points.is_empty() => {
                let from = d.points[0];
                if from.x != p.x && from.y != p.y {
                    self.commands.push((
                        "schematic.graphic.add_rect".to_owned(),
                        json!({"document": self.document, "from": at(from), "to": at(p)}),
                    ));
                    d.committed = outline(from, p);
                    d.points.clear();
                }
            }
            Tool::Line if d.points.last() == Some(&p) => self.finish_line(&mut d),
            _ => d.points.push(p),
        }
        d
    }

    /// Enter, or the last point clicked again: queues the polyline as one Command (a
    /// batch for several segments, so one undo step) and starts afresh.
    pub(super) fn finish_line(&mut self, d: &mut Drawing) {
        let segments: Vec<(Point, Point)> = d.points.windows(2).map(|w| (w[0], w[1])).collect();
        let lines: Vec<Value> = segments
            .iter()
            .map(|(a, b)| json!({"document": self.document, "from": at(*a), "to": at(*b)}))
            .collect();
        match lines.as_slice() {
            [] => {}
            [one] => self
                .commands
                .push(("schematic.graphic.add_line".to_owned(), one.clone())),
            _ => {
                let steps: Vec<Value> = lines
                    .into_iter()
                    .map(|params| json!({"id": "schematic.graphic.add_line", "params": params}))
                    .collect();
                self.commands
                    .push(("batch".to_owned(), json!({ "commands": steps })));
            }
        }
        d.committed = segments;
        d.points.clear();
    }
}
