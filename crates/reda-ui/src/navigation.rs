//! `view.jump_to`: Go To. It moves a canvas to an X/Y, absolute or as a delta from where
//! the canvas is centred, keeping the zoom. Absolute points are measured from design zero
//! or, on a board, from the Output Origin (design zero while none is placed).
//!
//! The window owns the cameras, so the Command queues a [`Request::JumpTo`] that the window
//! carries out with [`Shell::jump`](crate::shell::Shell::jump).

use reda_engine::command::{Command, CommandError, Context, Kind, Length, Meta, Preset};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{DocumentKind, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bars::Request;
use crate::bars::commands::request;
use crate::layout::Layout;

/// Where a canvas moves to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jump {
    /// Centre on this Document point.
    To(Point),
    /// Move the centre by this much.
    By(Point),
}

/// What absolute coordinates are measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JumpOrigin {
    /// Design zero.
    #[default]
    Design,
    /// The board's Output Origin, or design zero while none is placed. Boards only.
    Output,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JumpToParams {
    /// The Document, or open Library element, whose canvas to move; the active one when
    /// left out.
    #[serde(default)]
    document: Option<Uuid>,
    x: Length,
    y: Length,
    /// Move by `x` and `y` from where the canvas is centred, rather than to them.
    #[serde(default)]
    delta: bool,
    /// What `x` and `y` are measured from, unless `delta`.
    #[serde(default)]
    origin: JumpOrigin,
}

/// Where the canvas goes.
#[derive(Serialize, JsonSchema)]
pub struct JumpToResult {
    document: Uuid,
    /// The Document point the canvas centres on, for absolute jumps.
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<Point>,
    /// How far the canvas moves, for jumps by a delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    by: Option<Point>,
}

pub(crate) struct JumpTo;

impl Command for JumpTo {
    const META: Meta = Meta {
        id: "view.jump_to",
        title: "Go To",
        description: "Centres a Document's canvas, the active one by default, on an X/Y \
                      measured from design zero or, on a board, from the Output Origin; or \
                      moves it by X/Y with `delta`. The zoom stays.",
        category: "view",
        icon: "origin",
        kind: Kind::View,
    };
    type Params = JumpToParams;
    type Output = JumpToResult;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new(
                "design_zero",
                "Go to Design Zero",
                "origin",
                json!({"x": 0, "y": 0}),
            ),
            Preset::new(
                "output_origin",
                "Go to Output Origin",
                "origin",
                json!({"x": 0, "y": 0, "origin": "output"}),
            ),
        ]
    }

    fn run(&self, cx: &mut Context<'_>, p: JumpToParams) -> Result<JumpToResult, CommandError> {
        let document = match p.document {
            Some(document) => document,
            None => cx
                .view::<Layout>()?
                .active_document()
                .ok_or_else(|| CommandError::unavailable("no Document is active"))?,
        };
        let project = cx
            .project()
            .map_err(|_| CommandError::not_found(format!("no Document {document}")))?;
        let kind = project.document(document).map(|d| d.kind);
        if kind.is_none() && project.element(document).is_none() {
            return Err(CommandError::not_found(format!("no Document {document}")));
        }
        let point = Point::new(p.x.nm(), p.y.nm());
        if p.delta {
            request(cx, Request::JumpTo(document, Jump::By(point)))?;
            return Ok(JumpToResult {
                document,
                at: None,
                by: Some(point),
            });
        }
        let origin = match p.origin {
            JumpOrigin::Design => Point::new(Nm(0), Nm(0)),
            JumpOrigin::Output if kind == Some(DocumentKind::Pcb) => project
                .pcb_document(document)
                .and_then(|pcb| pcb.output_origin())
                .map_or(Point::new(Nm(0), Nm(0)), |o| o.at),
            JumpOrigin::Output => {
                return Err(CommandError::invalid_params(
                    "/origin",
                    "only a board has an Output Origin",
                ));
            }
        };
        let at = Point::new(origin.x + point.x, origin.y + point.y);
        request(cx, Request::JumpTo(document, Jump::To(at)))?;
        Ok(JumpToResult {
            document,
            at: Some(at),
            by: None,
        })
    }
}
