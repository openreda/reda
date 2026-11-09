//! The canvas widget: lays out the canvas, turns pointer input into camera moves and
//! Tool events, and adds the paint callback plus the overlays drawn over it.
//!
//! A frame is two calls with the editor between them: [`canvas_input`] lays the canvas
//! out and returns its pointer events, the editor turns them into Commands and updates the
//! tab's marks, and [`canvas_paint`] draws what the tab now shows. [`canvas`] does both
//! with no editor.

use std::sync::Arc;

use egui::{Color32, PointerButton, Rect, Role, Sense, Stroke, Ui, WidgetInfo};
use reda_engine::model::geom::Point;
use reda_engine::render::gpu::GridSpec;
use reda_engine::render::{ColorKey, Palette, SceneSource, SchematicColor, Theme};

use super::camera::{Camera, CanvasInput, grid_pitch};
use super::gpu::{CanvasFrame, paint};
use super::interact::{BoxMode, Pointer, pointer};
use crate::bars::DocumentType;
use crate::shell::DocumentTab;
use crate::tr;

/// The grid dots' radius in points.
const GRID_DOT_RADIUS: f32 = 1.0;
/// The canvas a Document is fitted to before it has any size: an A4 sheet.
const EMPTY_SHEET_NM: (i64, i64) = (297_000_000, 210_000_000);
/// A rubber-banded Wire's width: a sheet's Wire width (6 mil), at least this many points.
const MIN_WIRE_POINTS: f32 = 1.5;
const WIRE_NM: f64 = 152_400.0;

/// What a Tool draws over the canvas this frame, besides the marks on the Scene's objects.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// A selection box being dragged.
    pub rubber_band: Option<RubberBand>,
    /// Wires a drag in flight redraws, as (from, to).
    pub wires: Arc<[(Point, Point)]>,
    /// Wires a Tool proposes (Auto Wire's route), drawn as a thick dotted line.
    pub dotted: Arc<[(Point, Point)]>,
    /// Where the pointer snapped to.
    pub cursor: Option<Point>,
    /// The selected Wires' handles: (where, red), red for a selected segment's.
    pub handles: Arc<[(Point, bool)]>,
    /// Selected Wire segments, drawn over in the selection colour.
    pub segments: Arc<[(Point, Point)]>,
    /// The Break Wire Tool's preview of the cut under the pointer.
    pub cutter: Option<Cutter>,
    /// The Junctions a drag in flight would make (Display When Dragging).
    pub junctions: Arc<[Point]>,
}

/// What the Break Wire Tool shows of the cut it would make.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cutter {
    /// The stretch of Wire that would go.
    pub gap: (Point, Point),
    /// The Cutter Box around it, as four corners in order (Show Cutter Box).
    pub cutter_box: Option<[Point; 4]>,
    /// Extremity Markers across the Wire at both ends of the gap, half as long as the
    /// box is wide (Show Extremity Markers).
    pub extremities: Option<[(Point, Point); 2]>,
    /// With both displays Never: the gap itself is highlighted.
    pub highlight: bool,
}

/// The colour of a selected segment's handles.
pub const SEGMENT_HANDLE: Color32 = Color32::from_rgb(230, 40, 40);
/// A handle's half size in points.
const HANDLE_POINTS: f32 = 3.5;

/// A selection box being dragged out: its corners (nm) and how it picks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RubberBand {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub mode: BoxMode,
}

/// One frame of a canvas's layout and input.
pub struct CanvasInputs {
    pub rect: Rect,
    pub response: egui::Response,
    /// The canvas's camera after this frame's pan and zoom.
    pub camera: Camera,
    /// A board seen from the bottom: x runs right to left on screen.
    pub mirrored: bool,
    /// Primary-button events for the active Tool, in Document coordinates.
    pub events: Vec<Pointer>,
}

impl CanvasInputs {
    /// The Document point (nm) under the screen position `pos`.
    pub fn document_point(&self, pos: egui::Pos2) -> (f64, f64) {
        self.camera
            .document_point(self.rect, mirror(self.rect, pos, self.mirrored))
    }

    /// Where the Document point `p` (nm) shows on screen.
    pub fn screen_point(&self, p: (f64, f64)) -> egui::Pos2 {
        mirror(
            self.rect,
            self.camera.screen_point(self.rect, p),
            self.mirrored,
        )
    }
}

/// `pos` reflected about the vertical centre line of `rect` when `mirrored`.
/// How many of the selected objects `tab`'s canvas draws.
fn selected_on_canvas(tab: &DocumentTab) -> usize {
    let selected = &tab.selection.objects;
    if selected.is_empty() {
        return 0;
    }
    let mut drawn: Vec<_> = (tab.scene.scene.items.iter())
        .map(|item| {
            item.child
                .as_ref()
                .map(|c| c.id())
                .filter(|id| selected.contains(id))
                .unwrap_or(item.object)
        })
        .filter(|object| selected.contains(object))
        .collect();
    drawn.sort_unstable();
    drawn.dedup();
    drawn.len()
}

pub(crate) fn mirror(rect: Rect, pos: egui::Pos2, mirrored: bool) -> egui::Pos2 {
    if mirrored {
        egui::pos2(2.0 * rect.center().x - pos.x, pos.y)
    } else {
        pos
    }
}

/// Shows `tab`'s canvas filling the space left in `ui`, with no Tools.
pub fn canvas(ui: &mut Ui, tab: &mut DocumentTab, theme: Theme) {
    let kind = match tab.scene.scene.source {
        SceneSource::Pcb(_) => DocumentType::Pcb,
        _ => DocumentType::Schematic,
    };
    let input = canvas_input(ui, tab, kind);
    canvas_paint(ui, &input, tab, theme);
}

/// Lays out `tab`'s canvas, a Document of type `kind`, filling the space left in `ui`;
/// moves its camera, and returns the frame's pointer events.
///
/// The canvas is one accessible element: it names the Document and describes how many of
/// its objects are selected.
pub fn canvas_input(ui: &mut Ui, tab: &mut DocumentTab, kind: DocumentType) -> CanvasInputs {
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let title = tab.title.as_str();
    let name = match kind {
        DocumentType::Schematic => tr!("canvas-name-schematic", document = title),
        DocumentType::Pcb => tr!("canvas-name-pcb", document = title),
        DocumentType::Symbol => tr!("canvas-name-symbol", document = title),
        DocumentType::Footprint => tr!("canvas-name-footprint", document = title),
    };
    let selected = selected_on_canvas(tab);
    let description = match selected {
        0 => tr!("canvas-nothing-selected"),
        count => tr!("canvas-selected", count = count),
    };
    response.widget_info(|| WidgetInfo::labeled(Role::Canvas, true, name.as_str()));
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_description(description);
    });

    let mirrored = tab.mirrored();
    if let Some((lo, hi)) = tab.zoom_to.take() {
        tab.camera = Some(Camera::fit(lo, hi, rect.size()));
    }
    let camera = tab.camera.get_or_insert_with(|| {
        let (lo, hi) = tab.scene.scene.bounds().unwrap_or_else(|| {
            use reda_engine::model::geom::Nm;
            (
                Point::new(Nm(0), Nm(0)),
                Point::new(Nm(EMPTY_SHEET_NM.0), Nm(EMPTY_SHEET_NM.1)),
            )
        });
        Camera::fit(lo, hi, rect.size())
    });
    if let Some(at) = tab.center_on.take() {
        camera.center = (at.x.0 as f64, at.y.0 as f64);
    }

    let mut input = CanvasInput::default();
    if response.hovered() {
        input.pointer = response.hover_pos().map(|p| mirror(rect, p, mirrored));
        ui.input(|i| {
            input.scroll = i.smooth_scroll_delta.y;
            input.zoom = i.zoom_delta();
        });
    }
    if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
        input.pan = response.drag_delta();
        if mirrored {
            input.pan.x = -input.pan.x;
        }
    }
    camera.apply(rect, &input);
    let camera = *camera;
    let events = pointer(ui, &response, &camera, rect, mirrored);
    CanvasInputs {
        rect,
        response,
        camera,
        mirrored,
        events,
    }
}

/// Draws `tab` into the canvas laid out by [`canvas_input`].
pub fn canvas_paint(ui: &mut Ui, input: &CanvasInputs, tab: &DocumentTab, theme: Theme) {
    let (rect, camera) = (input.rect, input.camera);
    let ppp = ui.ctx().pixels_per_point();
    let mut view = camera.view(rect, ppp);
    view.mirrored = input.mirrored;
    let palette = tab.palette.palette(theme);
    let grid = grid_pitch(tab.grid.0 as f64, view.nm_per_px).map(|pitch| GridSpec {
        origin: (tab.grid_origin.x.0 as f64, tab.grid_origin.y.0 as f64),
        pitch,
        radius_px: GRID_DOT_RADIUS * ppp,
        color: palette.grid(),
        // Only a sheet has paper; a board's Scene is not searched for one every frame.
        paper: matches!(tab.scene.scene.source, SceneSource::Schematic(_))
            .then(|| tab.scene.scene.paper())
            .flatten()
            .map(|(lo, hi)| {
                (
                    (lo.x.0 as f64, lo.y.0 as f64),
                    (hi.x.0 as f64, hi.y.0 as f64),
                )
            }),
    });
    let frame = CanvasFrame {
        canvas: canvas_id(tab),
        scene: tab.scene.clone(),
        theme,
        view,
        grid,
        selection: tab.selection.clone(),
        palette: tab.palette.clone(),
        live: tab.live.clone(),
        moving_offset: tab.moving_offset,
    };
    paint(ui.painter(), rect, frame);
    overlay(ui, input, &tab.overlay, &palette);
    super::markers::draw(ui, input, tab, &palette);
}

/// Draws a Tool's overlay: rubber-banded Wires, the snap cursor and a selection box (solid
/// in the accent when it encloses, dashed in green when it touches).
fn overlay(ui: &Ui, input: &CanvasInputs, overlay: &Overlay, palette: &Palette) {
    let (rect, camera) = (input.rect, &input.camera);
    let painter = ui.painter_at(rect);
    let screen = |p: Point| input.screen_point((p.x.0 as f64, p.y.0 as f64));
    let wire = palette.color(ColorKey::Schematic(SchematicColor::Wire)).0;
    let wire = Color32::from_rgb(wire[0], wire[1], wire[2]);
    let width = ((WIRE_NM / camera.nm_per_point) as f32).max(MIN_WIRE_POINTS);
    for &(a, b) in overlay.wires.iter() {
        painter.line_segment([screen(a), screen(b)], Stroke::new(width, wire));
    }
    // Dots of twice the Wire's width, as far apart again as they are wide.
    let dot = width.max(2.0);
    for &(a, b) in overlay.dotted.iter() {
        let (a, b) = (screen(a), screen(b));
        let steps = ((b - a).length() / (dot * 2.0)).floor() as usize;
        for i in 0..=steps {
            let t = if steps == 0 {
                0.0
            } else {
                i as f32 / steps as f32
            };
            painter.circle_filled(a + (b - a) * t, dot, wire);
        }
    }
    let accent = ui.visuals().selection.stroke.color;
    for &(a, b) in overlay.segments.iter() {
        painter.line_segment([screen(a), screen(b)], Stroke::new(width + 2.0, accent));
    }
    if let Some(cutter) = overlay.cutter {
        let (a, b) = (screen(cutter.gap.0), screen(cutter.gap.1));
        if cutter.highlight {
            let hot = ui.visuals().warn_fg_color;
            painter.line_segment([a, b], Stroke::new(width + 3.0, hot));
        }
        if let Some(corners) = cutter.cutter_box {
            let points: Vec<egui::Pos2> = corners.iter().map(|&p| screen(p)).collect();
            painter.add(egui::Shape::closed_line(points, Stroke::new(1.0, accent)));
        }
        for (from, to) in cutter.extremities.into_iter().flatten() {
            painter.line_segment([screen(from), screen(to)], Stroke::new(2.0, accent));
        }
    }
    let dot = ((WIRE_NM * 3.0 / camera.nm_per_point) as f32).max(3.0);
    for &at in overlay.junctions.iter() {
        painter.circle_filled(screen(at), dot, wire);
    }
    for &(at, red) in overlay.handles.iter() {
        let fill = if red { SEGMENT_HANDLE } else { accent };
        let square = Rect::from_center_size(screen(at), egui::Vec2::splat(HANDLE_POINTS * 2.0));
        painter.rect_filled(square, 0.0, fill);
        painter.rect_stroke(
            square,
            0.0,
            Stroke::new(1.0, Color32::WHITE),
            egui::StrokeKind::Outside,
        );
    }
    if let Some(at) = overlay.cursor {
        let c = screen(at);
        let r = 6.0;
        let stroke = Stroke::new(1.0, accent);
        painter.line_segment([c - egui::vec2(r, 0.0), c + egui::vec2(r, 0.0)], stroke);
        painter.line_segment([c - egui::vec2(0.0, r), c + egui::vec2(0.0, r)], stroke);
    }
    if let Some(RubberBand {
        from: a,
        to: b,
        mode,
    }) = overlay.rubber_band
    {
        let band = Rect::from_two_pos(input.screen_point(a), input.screen_point(b));
        let color = crate::theme::selection_box(ui.ctx().theme(), mode == BoxMode::Touch);
        painter.rect_filled(band, 0.0, color.gamma_multiply(0.12));
        let stroke = Stroke::new(1.0, color);
        match mode {
            BoxMode::Enclose => {
                painter.rect_stroke(band, 0.0, stroke, egui::StrokeKind::Inside);
            }
            BoxMode::Touch => {
                let corners = [
                    band.left_top(),
                    band.right_top(),
                    band.right_bottom(),
                    band.left_bottom(),
                    band.left_top(),
                ];
                for pair in corners.windows(2) {
                    painter.extend(egui::Shape::dashed_line(pair, stroke, 4.0, 3.0));
                }
            }
        }
    }
}

/// The id a tab's canvas keeps its GPU state under.
pub fn canvas_id(tab: &DocumentTab) -> egui::Id {
    egui::Id::NULL.with(("canvas", tab.document))
}
