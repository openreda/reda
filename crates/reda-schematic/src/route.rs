//! Auto Wire: `schematic.wire.route` asks the in-house router ([`route`]) for a 90° path
//! between two points on a Schematic Sheet, around the objects placed there. It draws
//! nothing; the Wire Tool previews the path and commits it with `schematic.wire.add`.

mod search;
#[cfg(test)]
mod tests;

use reda_command::{Command, CommandError, Context, Kind, Length, Meta};
use reda_model::geom::{Nm, Point};
use reda_model::{SchematicSheet, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use search::{Avoid, Obstacles, Options, Route, RouteError, route};

use crate::component::At;
use crate::connectivity::placed_pins;
use crate::edit::bounds::unit_body;
use crate::sheet;

impl Obstacles {
    /// What a route on `body` goes around: each Unit's body with its Pins, every Pin end,
    /// Net Label, Port, Power Port and No-Connect point, and every Wire.
    pub fn of_sheet(body: &SchematicSheet) -> Obstacles {
        let mut points: Vec<Point> = placed_pins(body).into_iter().map(|(_, _, p)| p).collect();
        points.extend(body.net_labels().map(|l| l.at));
        points.extend(body.ports().map(|p| p.at));
        points.extend(body.power_ports().map(|p| p.at));
        points.extend(body.no_connects().map(|m| m.at));
        Obstacles {
            boxes: body.units().filter_map(|u| unit_body(body, u)).collect(),
            points,
            wires: body.wires().map(|w| w.vertices.clone()).collect(),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WireRouteParams {
    /// The Schematic Sheet to route on.
    pub document: Uuid,
    /// Where the route starts: the run's last vertex.
    pub from: At,
    /// Where it ends: the clicked point.
    pub to: At,
    /// The grid its corners sit on; 50 mil by default.
    pub grid: Option<Length>,
    /// How hard it tries not to cross Wires; `medium` by default.
    pub avoid_cutting_wires: Option<Avoid>,
    /// Time Out After, in milliseconds, 100 to 60000; 3000 by default. It buys a fixed
    /// amount of search, so the answer does not depend on the machine.
    pub timeout_ms: Option<u32>,
}

/// The route `schematic.wire.route` found.
#[derive(Debug, Serialize, JsonSchema)]
pub struct WireRouteResult {
    /// The path's vertices from `from` to `to`, corners only, ready for
    /// `schematic.wire.add`.
    pub points: Vec<Point>,
    /// How many Wires it crosses mid-span.
    pub crossings: u32,
}

pub struct WireRoute;

impl Command for WireRoute {
    const META: Meta = Meta {
        id: "schematic.wire.route",
        title: "Route Wire",
        description: "Finds a 90° path between two points on a Schematic Sheet for Auto \
                      Wire, around placed objects: it never enters a Unit's body or Pins, \
                      never passes over a Pin end, Net Label, Port, Power Port, No-Connect \
                      Marker or Wire vertex, never runs along or turns on a Wire, and \
                      crosses Wires only as `avoid_cutting_wires` allows. It prefers short \
                      paths with few corners, a grid step clear of bodies. The same inputs \
                      always give the same path. Nothing is drawn.",
        category: "schematic",
        icon: "wire",
        kind: Kind::Query,
    };
    type Params = WireRouteParams;
    type Output = WireRouteResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: WireRouteParams,
    ) -> Result<WireRouteResult, CommandError> {
        let body = sheet(cx.project()?, p.document)?;
        let grid = p.grid.map_or(Nm::from_mil(50), Length::nm);
        if grid.0 <= 0 {
            return Err(CommandError::invalid_params(
                "/grid",
                "the grid must be positive",
            ));
        }
        let timeout_ms = p.timeout_ms.unwrap_or(3000);
        if !(100..=60_000).contains(&timeout_ms) {
            return Err(CommandError::invalid_params(
                "/timeout_ms",
                "Time Out After is 100 to 60000 ms",
            ));
        }
        let options = Options {
            grid,
            avoid: p.avoid_cutting_wires.unwrap_or_default(),
            timeout_ms,
        };
        let (from, to) = (p.from.point(), p.to.point());
        match route(&Obstacles::of_sheet(body), from, to, &options) {
            Ok(r) => Ok(WireRouteResult {
                points: r.points,
                crossings: r.crossings,
            }),
            Err(RouteError::SamePoint) => Err(CommandError::invalid_params(
                "/to",
                "the route ends where it starts",
            )),
            Err(RouteError::BadGrid) => Err(CommandError::invalid_params(
                "/grid",
                "the grid must be positive",
            )),
            Err(RouteError::TooLarge) => Err(CommandError::rejected(
                "the ends are too far apart for this grid",
            )),
            Err(RouteError::NoRoute) => Err(CommandError::rejected(
                "no route gets round the objects between the two points",
            )),
            Err(RouteError::TimedOut) => Err(CommandError::rejected(
                "no route found before Time Out After ran out",
            )),
        }
    }
}
