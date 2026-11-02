//! The PCB canvas in a window: draws the board (through the shared canvas widget), feeds
//! the pointer and keys to the [`Tool`](super::Tool) and the view Commands, and draws what
//! egui adds on top: the rubber band, the snap marker, the cursor position, the context
//! menu and the "No Board Outline" hint.

use egui::{
    Align2, Color32, CornerRadius, Event, FontId, Id, Key, Modal, Modifiers, PointerButton, Pos2,
    Rect, Role, Stroke, StrokeKind, Ui, WidgetInfo, pos2, vec2,
};
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::render::Theme;
use serde_json::{Value, json};

use super::snap::SnapKind;
use super::view::format_length;
use crate::canvas::{CanvasShown, canvas};
use crate::layout::TabKey;
use crate::shell::Shell;
use crate::tr;

/// How near a click counts as on an object, in points.
const PICK_POINTS: f64 = 4.0;
/// How near electrical points catch the cursor, in points.
const SNAP_POINTS: f64 = 10.0;

/// The keys the board takes while its canvas has the focus, and the Command (or Preset)
/// each runs. The Keymap (#94) will take these over.
pub const VIEW_KEYS: &[(KeyChord, &str)] = &[
    (KeyChord::Text("+"), "pcb.layer.step:next"),
    (KeyChord::Text("-"), "pcb.layer.step:previous"),
    (KeyChord::Text("*"), "pcb.layer.swap_side"),
    (
        KeyChord::Key(Modifiers::SHIFT, Key::S),
        "pcb.view.inactive_layers",
    ),
    (KeyChord::Key(Modifiers::CTRL, Key::Q), "view.units.toggle"),
    (KeyChord::Key(Modifiers::NONE, Key::G), "view.grid.set"),
];

/// A key as the board binds it: a logical character, or a key with modifiers.
#[derive(Clone, Copy, Debug)]
pub enum KeyChord {
    Text(&'static str),
    Key(Modifiers, Key),
}

/// Shows the board `document`'s canvas filling `ui`, and handles its input.
pub fn show(ui: &mut Ui, shell: &mut Shell, document: Uuid, theme: Theme) {
    let grid = shell.board_view().grid;
    let Some(tab) = shell.document_tab_mut(document) else {
        return;
    };
    let shown = canvas(ui, tab, theme, Some(grid.0 as f64));
    let nm_per_point = shown.camera.nm_per_point;
    shell
        .pcb_tool_mut()
        .set_snap_radius(Nm((SNAP_POINTS * nm_per_point) as i64));

    pointer(ui, shell, &shown);
    if shell.layout().focus == Some(TabKey::Document(document)) {
        keys(ui, shell);
    }
    overlay(ui, shell, &shown);
    context_menu(shell, &shown);
    outline_hint(ui, shell, &shown);
}

fn board_point(shown: &CanvasShown, pos: Pos2) -> Point {
    let (x, y) = shown.document_point(pos);
    Point::new(Nm(x.round() as i64), Nm(y.round() as i64))
}

/// Left-button presses, drags and releases go to the Tool; with no button down the cursor
/// snaps for the marker.
fn pointer(ui: &Ui, shell: &mut Shell, shown: &CanvasShown) {
    let id = shown.response.id.with("pcb-pressed");
    let (pressed, down, released, pos, modifiers) = ui.input(|i| {
        (
            i.pointer.button_pressed(PointerButton::Primary),
            i.pointer.button_down(PointerButton::Primary),
            i.pointer.button_released(PointerButton::Primary),
            i.pointer.interact_pos(),
            i.modifiers,
        )
    });
    let mut ours: bool = ui.data(|d| d.get_temp(id).unwrap_or(false));
    let on_canvas =
        |p: Pos2| shown.rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id());
    if pressed && let Some(p) = pos.filter(|p| on_canvas(*p)) {
        let tolerance = Nm((PICK_POINTS * shown.camera.nm_per_point) as i64);
        shell.pcb_press(board_point(shown, p), modifiers.command, tolerance);
        ours = true;
    }
    if ours
        && down
        && shown.response.dragged_by(PointerButton::Primary)
        && let Some(p) = pos
    {
        shell.pcb_drag(board_point(shown, p));
    }
    if ours && released {
        shell.pcb_release();
        ours = false;
    }
    if !down && let Some(p) = shown.response.hover_pos() {
        shell.pcb_hover(board_point(shown, p));
    }
    ui.data_mut(|d| d.insert_temp(id, ours));
}

/// Space, L, Esc and the view keys, unless a text field has the keyboard.
fn keys(ui: &Ui, shell: &mut Shell) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let (rotate, flip, cancel, commands) = ui.input_mut(|i| {
        let rotate = i.consume_key(Modifiers::NONE, Key::Space);
        let flip = i.consume_key(Modifiers::NONE, Key::L);
        let cancel = i.consume_key(Modifiers::NONE, Key::Escape);
        let mut commands = Vec::new();
        for (chord, id) in VIEW_KEYS {
            let hit = match chord {
                KeyChord::Key(mods, key) => i.consume_key(*mods, *key),
                KeyChord::Text(text) => {
                    let before = i.events.len();
                    i.events
                        .retain(|e| !matches!(e, Event::Text(t) if t == text));
                    i.events.len() != before
                }
            };
            if hit {
                commands.push(*id);
            }
        }
        (rotate, flip, cancel, commands)
    });
    if rotate {
        shell.pcb_rotate();
    }
    if flip {
        shell.pcb_flip();
    }
    if cancel {
        shell.pcb_cancel();
    }
    for id in commands {
        let _ = shell.run_reporting(id, json!({}));
    }
}

/// The rubber band, the snap marker and the cursor position.
fn overlay(ui: &Ui, shell: &Shell, shown: &CanvasShown) {
    let painter = ui.painter_at(shown.rect);
    let accent = ui.visuals().selection.stroke.color;
    let screen = |p: Point| shown.screen_point((p.x.0 as f64, p.y.0 as f64));
    if let Some((a, b)) = shell.pcb_band() {
        let rect = Rect::from_two_pos(screen(a), screen(b));
        painter.rect(
            rect,
            CornerRadius::ZERO,
            accent.gamma_multiply(0.15),
            Stroke::new(1.0, accent),
            StrokeKind::Inside,
        );
    }
    let hovered = shown.response.hovered() || shell.pcb_tool().moving();
    if hovered && let Some((at, kind)) = shell.pcb_snap() {
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
    let view = shell.board_view();
    let text = match shell.pcb_snap().filter(|_| hovered) {
        Some((at, _)) => tr!(
            "pcb-cursor",
            x = format_length(at.x, view.units),
            y = format_length(at.y, view.units),
            grid = format_length(view.grid, view.units)
        ),
        None => tr!("pcb-grid", grid = format_length(view.grid, view.units)),
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

/// Right-click without dragging: Rotate, Flip Side, Lock and Unlock for the selection.
fn context_menu(shell: &mut Shell, shown: &CanvasShown) {
    let selected = !shell.selection().objects.is_empty();
    let mut clicked: Option<&str> = None;
    shown.response.context_menu(|ui| {
        for (title, action) in [
            (tr!("pcb-rotate"), "rotate"),
            (tr!("pcb-flip"), "flip"),
            (tr!("pcb-lock"), "lock"),
            (tr!("pcb-unlock"), "unlock"),
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

/// The form "Create rectangle…" opens: width and height as typed.
#[derive(Clone, Debug)]
struct RectForm {
    width: String,
    height: String,
}

/// While the board has no Board Outline: "No Board Outline: draw one, or Create
/// rectangle…", whose form runs `pcb.board_outline.add_rect`.
fn outline_hint(ui: &Ui, shell: &mut Shell, shown: &CanvasShown) {
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
