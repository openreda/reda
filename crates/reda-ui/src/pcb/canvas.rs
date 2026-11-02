//! The PCB canvas in a window. The shared canvas widget lays it out and turns the pointer
//! into [`Pointer`] events ([`crate::canvas::canvas_input`]); [`pointer`] feeds them to the
//! [`Tool`](super::Tool), and [`decorate`] draws what the board adds over the painted
//! canvas: the snap marker, the cursor position, the context menu and the "No Board
//! Outline" hint. Its keys are Keymap bindings in the `pcb` context (`keymap/reda.toml`).

use egui::{Align2, Color32, FontId, Id, Modal, Rect, Role, Stroke, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use serde_json::{Value, json};

use super::snap::SnapKind;
use super::view::format_length;
use crate::canvas::interact::{BoxMode, Pointer};
use crate::canvas::{CanvasInputs, RubberBand};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// How near a click counts as on an object, in points.
const PICK_POINTS: f64 = 4.0;
/// How near electrical points catch the cursor, in points.
const SNAP_POINTS: f64 = 10.0;

/// Feeds this frame's pointer events on the board `document`'s canvas to the Tool: a
/// press, drag and release (a click is a press and release), and with no button down the
/// cursor snaps for the marker. The Tool runs its Commands itself; its rubber band is the
/// tab's [`Overlay`](crate::canvas::Overlay), drawn as the sheet's is.
pub fn pointer(shell: &mut Shell, document: Uuid, input: &CanvasInputs) {
    let nm_per_point = input.camera.nm_per_point;
    shell.set_pcb_snap_radius(Nm((SNAP_POINTS * nm_per_point) as i64));
    shell
        .router_mut()
        .set_snap_radius(Nm((SNAP_POINTS * nm_per_point) as i64));
    if matches!(shell.tool(), Tool::Route | Tool::Via) {
        for &event in &input.events {
            match event {
                Pointer::Hover(at) | Pointer::Drag(at) | Pointer::DragStart { at, .. } => {
                    shell.route_hover(board_point(at))
                }
                Pointer::Click { at, .. } | Pointer::DragEnd(at) => {
                    shell.route_click(board_point(at))
                }
            }
        }
        if let Some(tab) = shell.document_tab_mut(document) {
            tab.overlay.rubber_band = None;
        }
        return;
    }
    let tolerance = Nm((PICK_POINTS * nm_per_point) as i64);
    for &event in &input.events {
        match event {
            Pointer::Hover(at) => {
                shell.pcb_hover(board_point(at));
            }
            Pointer::Click { at, modifiers } => {
                shell.pcb_press(board_point(at), modifiers.command, tolerance);
                shell.pcb_release();
            }
            Pointer::DragStart {
                from,
                at,
                modifiers,
            } => {
                shell.pcb_press(board_point(from), modifiers.command, tolerance);
                shell.pcb_drag(board_point(at));
            }
            Pointer::Drag(at) => shell.pcb_drag(board_point(at)),
            Pointer::DragEnd(at) => {
                shell.pcb_drag(board_point(at));
                shell.pcb_release();
            }
        }
    }
    let band = shell.pcb_band().map(|(a, b)| RubberBand {
        from: (a.x.0 as f64, a.y.0 as f64),
        to: (b.x.0 as f64, b.y.0 as f64),
        mode: BoxMode::Enclose,
    });
    if let Some(tab) = shell.document_tab_mut(document) {
        tab.overlay.rubber_band = band;
    }
}

/// Draws what the board adds over its painted canvas: the snap marker and cursor
/// position, the context menu and the "No Board Outline" hint.
pub fn decorate(ui: &Ui, shell: &mut Shell, input: &CanvasInputs) {
    super::route::draw(ui, shell, input);
    drawing(ui, shell, input);
    overlay(ui, shell, input);
    context_menu(shell, input);
    outline_hint(ui, shell, input);
    text_form(ui, shell, input);
}

/// What the drawing Tools show over the canvas: the shape in progress, a measurement,
/// the Mechanical Part being placed, and the user origin.
fn drawing(ui: &Ui, shell: &Shell, shown: &CanvasInputs) {
    let painter = ui.painter_at(shown.rect);
    let accent = ui.visuals().selection.stroke.color;
    let stroke = Stroke::new(1.5, accent);
    let screen = |p: Point| shown.screen_point((p.x.0 as f64, p.y.0 as f64));
    let draw = shell.pcb_draw();
    if let Some(preview) = draw.preview() {
        let mut points: Vec<_> = preview.points.iter().map(|p| screen(*p)).collect();
        if preview.closed
            && let Some(first) = points.first().copied()
        {
            points.push(first);
        }
        if points.len() > 1 {
            painter.add(egui::Shape::line(points.clone(), stroke));
        }
        for p in &points {
            painter.circle_stroke(*p, 3.0, Stroke::new(1.0, accent));
        }
        if let (Some(_), Some(at)) = (draw.part(), points.first()) {
            painter.text(
                *at + vec2(10.0, -10.0),
                Align2::LEFT_BOTTOM,
                tr!("pcb-placing"),
                FontId::proportional(12.0),
                accent,
            );
        }
    }
    if let Some(m) = draw.measurement() {
        let (a, b) = (screen(m.from), screen(m.to));
        painter.line_segment([a, b], stroke);
        painter.circle_stroke(a, 3.0, Stroke::new(1.0, accent));
        painter.circle_stroke(b, 3.0, Stroke::new(1.0, accent));
        let units = shell.board_view().units;
        let label = tr!(
            "pcb-measure",
            dx = format_length(m.to.x - m.from.x, units),
            dy = format_length(m.to.y - m.from.y, units),
            distance = format_length(m.distance(), units)
        );
        painter.text(
            b + vec2(8.0, -8.0),
            Align2::LEFT_BOTTOM,
            label,
            FontId::proportional(12.0),
            accent,
        );
    }
    if let Some(origin) = shell.board_view().origin {
        let c = screen(origin);
        let r = 7.0;
        painter.circle_stroke(c, r * 0.6, Stroke::new(1.0, accent));
        painter.line_segment(
            [c - vec2(r, 0.0), c + vec2(r, 0.0)],
            Stroke::new(1.0, accent),
        );
        painter.line_segment(
            [c - vec2(0.0, r), c + vec2(0.0, r)],
            Stroke::new(1.0, accent),
        );
    }
}

/// While text waits for its words: a small form that places it.
fn text_form(ui: &Ui, shell: &mut Shell, shown: &CanvasInputs) {
    if shell.pcb_draw().pending_text().is_none() {
        return;
    }
    let id = Id::NULL.with(("pcb-text-form", shown.response.id));
    let mut words = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    let (mut place, mut cancel) = (false, false);
    let modal = Modal::new(id.with("modal")).show(ui.ctx(), |ui| {
        ui.heading(tr!("pcb-text-title"));
        let field = ui.text_edit_singleline(&mut words);
        field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr!("pcb-text-words")));
        field.request_focus();
        if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            place = true;
        }
        ui.horizontal(|ui| {
            if ui.button(tr!("pcb-text-place")).clicked() {
                place = true;
            }
            if ui.button(tr!("pcb-text-cancel")).clicked() {
                cancel = true;
            }
        });
    });
    if modal.should_close() {
        cancel = true;
    }
    if place {
        shell.pcb_draw_text(&words);
    } else if cancel {
        shell.pcb_cancel_text();
    }
    ui.data_mut(|d| {
        if place || cancel {
            d.remove::<String>(id);
        } else {
            d.insert_temp(id, words);
        }
    });
}

fn board_point((x, y): (f64, f64)) -> Point {
    Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
}

/// The snap marker and the cursor position.
fn overlay(ui: &Ui, shell: &Shell, shown: &CanvasInputs) {
    let painter = ui.painter_at(shown.rect);
    let accent = ui.visuals().selection.stroke.color;
    let screen = |p: Point| shown.screen_point((p.x.0 as f64, p.y.0 as f64));
    let hovered = shown.response.hovered()
        || shell.pcb_tool().moving()
        || shell.pcb_draw().preview().is_some();
    let snapped = match shell.tool() {
        Tool::Route | Tool::Via => shell.router().snap(),
        _ => shell.pcb_snap(),
    };
    if hovered && let Some((at, kind)) = snapped {
        let c = screen(at);
        match kind {
            SnapKind::Grid => {
                let r = 4.0;
                painter.line_segment(
                    [c - vec2(r, 0.0), c + vec2(r, 0.0)],
                    Stroke::new(1.0, accent),
                );
                painter.line_segment(
                    [c - vec2(0.0, r), c + vec2(0.0, r)],
                    Stroke::new(1.0, accent),
                );
            }
            _ => {
                painter.circle_stroke(c, 6.0, Stroke::new(2.0, accent));
            }
        }
    }
    let units = shell.board_view().units;
    let grid = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map_or(crate::grid::pcb_default(units), |pcb| shell.grid(pcb.uuid));
    let text = match snapped.filter(|_| hovered) {
        Some((at, _)) => {
            let at = shell.pcb_cursor_from_origin(at);
            tr!(
                "pcb-cursor",
                x = format_length(at.x, units),
                y = format_length(at.y, units),
                grid = format_length(grid, units)
            )
        }
        None => tr!("pcb-grid", grid = format_length(grid, units)),
    };
    let font = FontId::proportional(12.0);
    let at = shown.rect.left_bottom() + vec2(8.0, -8.0);
    let galley = painter.layout_no_wrap(text, font, ui.visuals().text_color());
    let back = Rect::from_min_size(
        at - vec2(4.0, galley.size().y + 2.0),
        galley.size() + vec2(8.0, 4.0),
    );
    painter.rect_filled(back, 3.0, Color32::from_black_alpha(140));
    painter.galley(back.min + vec2(4.0, 2.0), galley, Color32::WHITE);
}

/// Right-click without dragging: Rotate, Flip Side, Lock, Unlock and Board Outline from
/// Selection for the selection.
fn context_menu(shell: &mut Shell, shown: &CanvasInputs) {
    let selected = !shell.selection().objects.is_empty();
    let mut clicked: Option<&str> = None;
    shown.response.context_menu(|ui| {
        for (title, action) in [
            (tr!("pcb-rotate"), "rotate"),
            (tr!("pcb-flip"), "flip"),
            (tr!("pcb-lock"), "lock"),
            (tr!("pcb-unlock"), "unlock"),
            (tr!("pcb-outline-from-selection"), "outline"),
        ] {
            if ui.add_enabled(selected, egui::Button::new(title)).clicked() {
                clicked = Some(action);
                ui.close();
            }
        }
    });
    match clicked {
        Some("rotate") => shell.pcb_rotate(),
        Some("flip") => shell.pcb_flip(),
        Some("outline") => {
            let graphics = selected_graphics(shell);
            if !graphics.is_empty() {
                let _ = shell.run_reporting(
                    "pcb.board_outline.add_from_graphics",
                    json!({ "graphics": graphics }),
                );
            }
        }
        Some(action) => {
            let footprints = selected_footprints(shell);
            if !footprints.is_empty() {
                let locked = action == "lock";
                let _ = shell.run_reporting(
                    "pcb.lock",
                    json!({"footprints": footprints, "locked": locked}),
                );
            }
        }
        None => {}
    }
}

fn selected_footprints(shell: &Shell) -> Vec<Uuid> {
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return Vec::new();
    };
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| pcb.footprint(*o).is_some())
        .collect()
}

fn selected_graphics(shell: &Shell) -> Vec<Uuid> {
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return Vec::new();
    };
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| pcb.graphic(*o).is_some())
        .collect()
}

/// The form "Create rectangle…" opens: width and height as typed.
#[derive(Clone, Debug)]
struct RectForm {
    width: String,
    height: String,
}

/// While the board has no Board Outline: "No Board Outline: draw one, or Create
/// rectangle…", whose form runs `pcb.board_outline.add_rect`.
fn outline_hint(ui: &Ui, shell: &mut Shell, shown: &CanvasInputs) {
    let has_outline = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .is_some_and(|pcb| pcb.outline().is_some());
    if has_outline {
        return;
    }
    let form_id = Id::NULL.with(("pcb-outline-rect", shown.response.id));
    let area = egui::Area::new(Id::NULL.with(("pcb-outline-hint", shown.response.id)))
        .order(egui::Order::Middle)
        .fixed_pos(pos2(shown.rect.center().x, shown.rect.top() + 56.0))
        .pivot(Align2::CENTER_TOP);
    area.show(ui.ctx(), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("pcb-no-outline"));
                let button = ui.button(tr!("pcb-create-rect"));
                button.widget_info(|| {
                    WidgetInfo::labeled(Role::Button, true, tr!("pcb-create-rect"))
                });
                if button.clicked() {
                    let units = shell.board_view().units;
                    let (w, h) = match units {
                        reda_engine::preferences::Units::Mm => ("100mm", "80mm"),
                        reda_engine::preferences::Units::Mil => ("4000mil", "3000mil"),
                    };
                    ui.data_mut(|d| {
                        d.insert_temp(
                            form_id,
                            RectForm {
                                width: w.to_owned(),
                                height: h.to_owned(),
                            },
                        )
                    });
                }
            });
        });
    });
    let Some(mut form) = ui.data(|d| d.get_temp::<RectForm>(form_id)) else {
        return;
    };
    let mut close = false;
    let mut create: Option<Value> = None;
    let modal = Modal::new(form_id.with("modal")).show(ui.ctx(), |ui| {
        ui.heading(tr!("pcb-outline-title"));
        egui::Grid::new(form_id.with("grid"))
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(tr!("pcb-outline-width"));
                let w = ui.text_edit_singleline(&mut form.width);
                w.widget_info(|| {
                    WidgetInfo::labeled(Role::TextInput, true, tr!("pcb-outline-width"))
                });
                ui.end_row();
                ui.label(tr!("pcb-outline-height"));
                let h = ui.text_edit_singleline(&mut form.height);
                h.widget_info(|| {
                    WidgetInfo::labeled(Role::TextInput, true, tr!("pcb-outline-height"))
                });
                ui.end_row();
            });
        ui.horizontal(|ui| {
            if ui.button(tr!("pcb-outline-create")).clicked() {
                let units = shell.board_view().units;
                create = Some(json!({
                    "width": with_unit(&form.width, units),
                    "height": with_unit(&form.height, units),
                }));
            }
            if ui.button(tr!("pcb-outline-cancel")).clicked() {
                close = true;
            }
        });
    });
    if modal.should_close() {
        close = true;
    }
    if let Some(params) = create
        && shell
            .run_reporting("pcb.board_outline.add_rect", params)
            .is_ok()
    {
        close = true;
    }
    ui.data_mut(|d| {
        if close {
            d.remove::<RectForm>(form_id);
        } else {
            d.insert_temp(form_id, form);
        }
    });
}

/// `text` as a length param: a bare number is taken in `units`.
fn with_unit(text: &str, units: reda_engine::preferences::Units) -> String {
    let text = text.trim();
    if text.parse::<f64>().is_ok() {
        match units {
            reda_engine::preferences::Units::Mm => format!("{text}mm"),
            reda_engine::preferences::Units::Mil => format!("{text}mil"),
        }
    } else {
        text.to_owned()
    }
}
