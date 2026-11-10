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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DocumentParams {
    /// The Document, or open Library element, whose canvas to move; the active one when
    /// left out.
    #[serde(default)]
    document: Option<Uuid>,
}

/// The canvas a navigation Command moves.
#[derive(Serialize, JsonSchema)]
pub struct DocumentResult {
    document: Uuid,
}

fn canvas_of(cx: &mut Context<'_>, document: Option<Uuid>) -> Result<Uuid, CommandError> {
    match document {
        Some(document) => Ok(document),
        None => cx
            .view::<Layout>()?
            .active_document()
            .ok_or_else(|| CommandError::unavailable("no Document is active")),
    }
}

/// `view.recenter` (Home): centres a canvas on the cursor, keeping the zoom.
pub(crate) struct Recenter;

impl Command for Recenter {
    const META: Meta = Meta {
        id: "view.recenter",
        title: "Re-Center on Cursor",
        description: "Centres a Document's canvas, the active one by default, on where the \
                      pointer last was over it, keeping the zoom.",
        category: "view",
        icon: "origin",
        kind: Kind::View,
    };
    type Params = DocumentParams;
    type Output = DocumentResult;

    fn run(&self, cx: &mut Context<'_>, p: DocumentParams) -> Result<DocumentResult, CommandError> {
        let document = canvas_of(cx, p.document)?;
        request(cx, Request::Recenter(document))?;
        Ok(DocumentResult { document })
    }
}

/// `view.previous` (Alt+←): puts a canvas back to the view it had before its last jump,
/// zoom or pan.
pub(crate) struct PreviousView;

impl Command for PreviousView {
    const META: Meta = Meta {
        id: "view.previous",
        title: "Previous View",
        description: "Puts a Document's canvas, the active one by default, back to the view \
                      it had before its last zoom, pan or jump; repeated, it steps further \
                      back.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = DocumentParams;
    type Output = DocumentResult;

    fn run(&self, cx: &mut Context<'_>, p: DocumentParams) -> Result<DocumentResult, CommandError> {
        let document = canvas_of(cx, p.document)?;
        request(cx, Request::PreviousView(document))?;
        Ok(DocumentResult { document })
    }
}

/// Where `view.selection.next` is in the selection. View state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectionStep {
    /// The selection it steps through, in pick order.
    objects: Vec<Uuid>,
    index: usize,
}

/// The object `view.selection.next` zoomed to.
#[derive(Serialize, JsonSchema)]
pub struct NextSelectedResult {
    object: Uuid,
    /// Its place in the selection, from 0.
    index: usize,
    count: usize,
    document: Uuid,
}

/// `view.selection.next`: steps through the selection, zooming to each object.
pub(crate) struct NextSelected;

impl Command for NextSelected {
    const META: Meta = Meta {
        id: "view.selection.next",
        title: "Next Selected Object",
        description: "Zooms to the next selected object, in pick order, opening its Document \
                      when it is on another one; after the last it starts again. A new \
                      selection starts at its first object. It has no default key.",
        category: "view",
        icon: "zoom_fit",
        kind: Kind::View,
    };
    type Params = reda_engine::command::NoParams;
    type Output = NextSelectedResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        _: reda_engine::command::NoParams,
    ) -> Result<NextSelectedResult, CommandError> {
        let selection = cx.view::<crate::selection::Selection>()?.clone();
        let mut objects: Vec<Uuid> = Vec::new();
        let picks = selection.objects.iter().copied();
        for o in picks.chain(selection.pads.iter().map(|p| p.footprint)) {
            if !objects.contains(&o) {
                objects.push(o);
            }
        }
        if objects.is_empty() {
            return Err(CommandError::unavailable("nothing is selected"));
        }
        let step = cx.view::<SelectionStep>()?;
        let index = if step.objects == objects {
            (step.index + 1) % objects.len()
        } else {
            0
        };
        *step = SelectionStep {
            objects: objects.clone(),
            index,
        };
        let object = objects[index];
        let home = cx
            .project()
            .ok()
            .and_then(|p| p.object(object))
            .and_then(|o| match o.scope() {
                reda_engine::model::Scope::Document(d) => Some(d),
                _ => None,
            });
        let layout = cx.view::<Layout>()?;
        let document = match home {
            Some(d) => {
                if layout.active_document() != Some(d) {
                    layout.open_document(d);
                }
                d
            }
            None => layout
                .active_document()
                .ok_or_else(|| CommandError::unavailable("no Document is active"))?,
        };
        request(cx, Request::ZoomTo(document, vec![object]))?;
        Ok(NextSelectedResult {
            object,
            index,
            count: objects.len(),
            document,
        })
    }
}
