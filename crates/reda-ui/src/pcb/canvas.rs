//! The PCB canvas in a window. The shared canvas widget lays it out and turns the pointer
//! into [`Pointer`] events ([`crate::canvas::canvas_input`]); [`pointer`] feeds them to the
//! [`Tool`](super::Tool), and [`decorate`] draws what the board adds over the painted
//! canvas: the snap marker, the cursor position, the context menu and the "No Board
//! Outline" hint. Its keys are Keymap bindings in the `pcb` context (`keymap/reda.toml`).

use egui::{Align2, Color32, FontId, Id, Rect, Role, Stroke, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use serde_json::{Value, json};

use super::view::format_length;
use crate::canvas::interact::Pointer;
use crate::canvas::{CanvasInputs, RubberBand};
use crate::shell::Shell;
use crate::tools::Tool;
use crate::tr;

/// How near a click counts as on an object, in points.
const PICK_POINTS: f64 = 5.0;

/// Feeds this frame's pointer events on the board `document`'s canvas to the Tool: a
/// press, drag and release (a click is a press and release), and with no button down the
/// cursor snaps for the marker. The Tool runs its Commands itself; its rubber band is the
/// tab's [`Overlay`](crate::canvas::Overlay), drawn as the sheet's is.
pub fn pointer(shell: &mut Shell, document: Uuid, input: &CanvasInputs) {
    let nm_per_point = input.camera.nm_per_point;
    let tolerance = Nm((PICK_POINTS * nm_per_point) as i64);
    if let Some(at) = input.events.iter().rev().find_map(|e| match e {
        Pointer::Hover(at) => Some(*at),
        _ => None,
    }) {
        let net = live_hover_net(shell, document, at, tolerance.0 as f64);
        shell.set_live_hover_candidate(net);
    }
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
    if shell.tool() == Tool::Select
        && let Some(commands) = crate::selection::picking::text_pointer(
            shell,
            document,
            &input.events,
            tolerance.0 as f64,
        )
    {
        for (id, params) in commands {
            let _ = shell.run_reporting(&id, params);
        }
        return;
    }
    for &event in &input.events {
        match event {
            Pointer::Hover(at) => {
                crate::selection::picking::hover(shell, document, at, tolerance.0 as f64);
                shell.pcb_hover(board_point(at));
            }
            Pointer::Click { at, modifiers } => {
                shell.pcb_press_with_modifiers(board_point(at), modifiers, tolerance);
                shell.pcb_release();
            }
            Pointer::DragStart {
                from,
                at,
                modifiers,
            } => {
                shell.pcb_press_with_modifiers(board_point(from), modifiers, tolerance);
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
        mode: crate::selection::rules::box_mode(
            shell,
            document,
            (a.x.0 as f64, a.y.0 as f64),
            (b.x.0 as f64, b.y.0 as f64),
        ),
    });
    if let Some(tab) = shell.document_tab_mut(document) {
        tab.overlay.rubber_band = band;
    }
}

/// The Net of what is under the board point `at` (nm) on `document`, for live hover.
fn live_hover_net(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    let hit = crate::selection::picking::hit(shell, document, at, tolerance)?;
    let pcb = shell.engine().project()?.pcb()?;
    let tab = shell.document_tab(document)?;
    super::live_hover::net_at(pcb, &tab.scene.scene, hit)
}

/// Draws what the board adds over its painted canvas: the snap marker and cursor
/// position, the context menu, the "No Board Outline" hint and the Layer control, whose
/// clicks it returns for the window to run.
pub fn decorate(ui: &Ui, shell: &mut Shell, input: &CanvasInputs) -> Vec<(String, Value)> {
    super::route::draw(ui, shell, input);
    super::live_hover::draw(ui, shell, input);
    drawing(ui, shell, input);
    overlay(ui, shell, input);
    let mut out = Vec::new();
    if super::route::takes_right_click(shell) {
        // A right-click ends the route instead of opening the menu.
        if input.response.secondary_clicked() {
            out.push(("pcb.route.end".to_owned(), json!({})));
        }
    } else {
        context_menu(shell, input);
    }
    outline_hint(ui, shell, input);
    text_form(ui, shell, input);
    out.extend(super::layer_control::show(ui, shell, input.response.id, input.rect));
    out.extend(super::routing_bar::show(ui, shell, input.response.id, input.rect));
    out
}

/// What the drawing Tools show over the canvas: the shape in progress, a measurement,
/// and the Mechanical Part being placed. (The Output Origin is in the Scene.)
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
}

/// While text waits for its words: a small form that places it.
fn text_form(ui: &Ui, shell: &mut Shell, shown: &CanvasInputs) {
    if shell.pcb_draw().pending_text().is_none() {
        return;
    }
    let id = Id::NULL.with(("pcb-text-form", shown.response.id));
    let mut words = ui.data(|d| d.get_temp::<String>(id)).unwrap_or_default();
    let (mut place, mut cancel) = (false, false);
    let modal =
        crate::modal::Dialog::new(id.with("modal"), tr!("pcb-text-title")).show(ui.ctx(), |ui| {
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
    if hovered && let Some(snapped) = snapped {
        crate::snap::marker(&painter, screen(snapped.at), snapped.hit, accent);
    }
    let units = shell.board_view().units;
    let grid = shell
        .engine()
        .project()
        .and_then(|p| p.pcb())
        .map_or(crate::grid::pcb_default(units), |pcb| shell.grid(pcb.uuid));
    let text = match snapped.filter(|_| hovered) {
        Some(snapped) => {
            let at = shell.pcb_cursor_from_origin(snapped.at);
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
    let strip = super::layer_control::strip_shown(shell);
    let at = crate::bars::draw::grid_readout_anchor(shown.rect, strip);
    let galley = painter.layout_no_wrap(text, font, ui.visuals().text_color());
    let back = Rect::from_min_size(
        at - vec2(4.0, galley.size().y + 2.0),
        galley.size() + vec2(8.0, 4.0),
    );
    painter.rect_filled(back, 3.0, Color32::from_black_alpha(140));
    let text = galley.text().to_owned();
    painter.galley(back.min + vec2(4.0, 2.0), galley, Color32::WHITE);
    let readout = ui.interact(
        back,
        shown.response.id.with("grid-readout"),
        egui::Sense::hover(),
    );
    readout.widget_info(|| {
        let mut info = WidgetInfo::labeled(Role::Group, true, tr!("pcb-grid-readout"));
        info.current_text_value = Some(text.clone());
        info
    });
}

/// What the board's context menu offers for the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuAction {
    Rotate,
    Flip,
    Lock,
    Unlock,
    Outline,
}

/// Right-click without dragging: Rotate, Flip Side, Lock, Unlock and Board Outline from
/// Selection for the selection, each through its Command.
fn context_menu(shell: &mut Shell, shown: &CanvasInputs) {
    let selected = !shell.selection().objects.is_empty();
    let mut clicked: Option<MenuAction> = None;
    shown.response.context_menu(|ui| {
        for (title, action) in [
            (tr!("pcb-rotate"), MenuAction::Rotate),
            (tr!("pcb-flip"), MenuAction::Flip),
            (tr!("pcb-lock"), MenuAction::Lock),
            (tr!("pcb-unlock"), MenuAction::Unlock),
            (tr!("pcb-outline-from-selection"), MenuAction::Outline),
        ] {
            if ui.add_enabled(selected, egui::Button::new(title)).clicked() {
                clicked = Some(action);
                ui.close();
            }
        }
    });
    let Some(action) = clicked else {
        return;
    };
    match action {
        MenuAction::Rotate => {
            let _ = shell.run_reporting("tool.rotate", json!({}));
        }
        MenuAction::Flip => {
            let _ = shell.run_reporting("tool.flip", json!({}));
        }
        MenuAction::Outline => {
            let graphics = selected_graphics(shell);
            if !graphics.is_empty() {
                let _ = shell.run_reporting(
                    "pcb.board_outline.add_from_graphics",
                    json!({ "graphics": graphics }),
                );
            }
        }
        MenuAction::Lock | MenuAction::Unlock => {
            let objects = selected_lockable(shell);
            if !objects.is_empty() {
                let locked = action == MenuAction::Lock;
                let _ =
                    shell.run_reporting("pcb.lock", json!({"objects": objects, "locked": locked}));
            }
        }
    }
}

/// The selected objects that can be locked: every placed kind.
fn selected_lockable(shell: &Shell) -> Vec<Uuid> {
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return Vec::new();
    };
    shell
        .selection()
        .objects
        .iter()
        .copied()
        .filter(|o| reda_engine::pcb::lock::lock_of(pcb, *o).is_some())
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
        // Clear of the Active Bar, which floats at the canvas's top.
        .fixed_pos(pos2(
            shown.rect.center().x,
            shown.rect.top() + crate::bars::draw::ACTIVE_BAR_BOTTOM + 12.0,
        ))
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
    let modal = crate::modal::Dialog::new(form_id.with("modal"), tr!("pcb-outline-title")).show(
        ui.ctx(),
        |ui| {
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
        },
    );
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
