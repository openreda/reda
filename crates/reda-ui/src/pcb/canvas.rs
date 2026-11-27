//! The PCB canvas in a window. The shared canvas widget lays it out and turns the pointer
//! into [`Pointer`] events ([`crate::canvas::canvas_input`]); [`pointer`] feeds them to the
//! [`Tool`](super::Tool), and [`decorate`] draws what the board adds over the painted
//! canvas: the Tools' marks ([`super::marks`]), the grid readout and the "No Board
//! Outline" hint. Its keys are Keymap bindings in the `pcb` context (`keymap/reda.toml`).

use egui::{Align2, Id, Rect, Role, Stroke, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use serde_json::{Value, json};

use super::view::format_length;
use crate::canvas::interact::Pointer;
use crate::canvas::{CanvasInputs, RubberBand};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tools::Tool;
use crate::tr;
use crate::widgets;

/// How near a click counts as on an object, in points.
const PICK_POINTS: f64 = 5.0;

/// The Commands of the wheel's Layer steps this frame (Step Layer in the wheel mapping):
/// one `pcb.layer.step` per step, next for the wheel turned towards the user.
pub fn wheel_layer_steps(input: &CanvasInputs) -> Vec<(String, Value)> {
    let direction = if input.layer_steps > 0 {
        "next"
    } else {
        "previous"
    };
    (0..input.layer_steps.unsigned_abs())
        .map(|_| {
            (
                "pcb.layer.step".to_owned(),
                json!({ "direction": direction }),
            )
        })
        .collect()
}

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
/// position, the "No Board Outline" hint and the Layer control, whose
/// clicks it returns for the window to run.
pub fn decorate(ui: &Ui, shell: &mut Shell, input: &CanvasInputs) -> Vec<(String, Value)> {
    super::route::draw(ui, shell, input);
    super::live_hover::draw(ui, shell, input);
    super::marks::drawing(ui, shell, input);
    super::polygon_edit::draw(ui, shell, input);
    super::dimension_edit::draw(ui, shell, input);
    super::copper_drag::draw(ui, shell, input);
    super::marks::snap_marker(ui, shell, input);
    grid_readout(ui, shell, input);
    let mut out = Vec::new();
    // A right-click ends the route instead of opening the menu (the window leaves the
    // canvas menu out while routing, see `context_menu::canvas`).
    if super::route::takes_right_click(shell) && input.response.secondary_clicked() {
        let end = match shell.tool() {
            Tool::Measure => "tool.activate:select",
            _ => "pcb.route.end",
        };
        out.push((end.to_owned(), json!({})));
    }
    outline_hint(ui, shell, input);
    text_form(ui, shell, input);
    super::layer_control::picker(ui, shell, &input.response, &mut out);
    out.extend(super::layer_control::show(
        ui,
        shell,
        input.response.id,
        input.rect,
    ));
    super::marks::move_marks(ui, shell, input);
    out.extend(super::placement_bar::show(
        ui,
        shell,
        input.response.id,
        input.rect,
    ));
    out.extend(super::routing_bar::show(
        ui,
        shell,
        input.response.id,
        input.rect,
    ));
    out
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
            let field = ui.add(widgets::text_field(&mut words));
            field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr!("pcb-text-words")));
            field.request_focus();
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                place = true;
            }
            crate::modal::buttons(ui, |ui| {
                if ui
                    .add(widgets::button(tr!("pcb-text-place")).primary())
                    .clicked()
                {
                    place = true;
                }
                if ui.add(widgets::button(tr!("pcb-text-cancel"))).clicked() {
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

/// The grid readout at the canvas's bottom left: the cursor position and the grid, or
/// the grid alone while the cursor is off the canvas. A small floating chip in the
/// numeric style.
fn grid_readout(ui: &Ui, shell: &Shell, shown: &CanvasInputs) {
    let t = ui.tokens();
    let painter = ui.painter_at(shown.rect);
    let units = shell.board_view().units;
    let grid = shell.engine().project().and_then(|p| p.pcb()).map_or(
        crate::grid::Step::square(crate::grid::pcb_default(units)),
        |pcb| shell.grid(pcb.uuid),
    );
    let snapped = super::marks::snapped(shell).filter(|_| super::marks::snap_shown(shell, shown));
    let text = match snapped {
        Some(snapped) => {
            let at = shell.pcb_cursor_from_origin(snapped.at);
            tr!(
                "pcb-cursor",
                x = format_length(at.x, units),
                y = format_length(at.y, units),
                grid = super::view::format_step(grid, units)
            )
        }
        None => tr!("pcb-grid", grid = super::view::format_step(grid, units)),
    };
    let strip = super::layer_control::strip_inset(ui, shell);
    let at = crate::bars::draw::grid_readout_anchor(shown.rect, strip);
    let galley = painter.layout_job(t.ty.numeric_small.job(&text, t.color.text.primary));
    let pad = vec2(t.space.row_x, t.space.row_y);
    let back = Rect::from_min_size(
        at - vec2(0.0, galley.size().y + 2.0 * pad.y),
        galley.size() + 2.0 * pad,
    );
    painter.rect(
        back,
        t.radius.row,
        t.color.surface.floating,
        Stroke::new(t.stroke.hairline, t.color.stroke.hairline),
        egui::StrokeKind::Inside,
    );
    painter.galley(back.min + pad, galley, t.color.text.primary);
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
            shown.rect.top()
                + crate::bars::draw::active_bar_bottom(&crate::theme::TokensExt::tokens(ui))
                + 12.0,
        ))
        .pivot(Align2::CENTER_TOP);
    let t = ui.tokens();
    area.show(ui.ctx(), |ui| {
        widgets::floating_frame(&t).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    t.ty.body
                        .rich(tr!("pcb-no-outline"))
                        .color(t.color.text.primary),
                );
                let button = ui.add(widgets::button(tr!("pcb-create-rect")));
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
            widgets::form(form_id.with("grid")).show(ui, |ui| {
                widgets::form_row(ui, tr!("pcb-outline-width"), |ui| {
                    let w = ui.add(widgets::text_field(&mut form.width));
                    w.widget_info(|| {
                        WidgetInfo::labeled(Role::TextInput, true, tr!("pcb-outline-width"))
                    });
                });
                widgets::form_row(ui, tr!("pcb-outline-height"), |ui| {
                    let h = ui.add(widgets::text_field(&mut form.height));
                    h.widget_info(|| {
                        WidgetInfo::labeled(Role::TextInput, true, tr!("pcb-outline-height"))
                    });
                });
            });
            crate::modal::buttons(ui, |ui| {
                if ui
                    .add(widgets::button(tr!("pcb-outline-create")).primary())
                    .clicked()
                {
                    let units = shell.board_view().units;
                    create = Some(json!({
                        "width": with_unit(&form.width, units),
                        "height": with_unit(&form.height, units),
                    }));
                }
                if ui.add(widgets::button(tr!("pcb-outline-cancel"))).clicked() {
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
