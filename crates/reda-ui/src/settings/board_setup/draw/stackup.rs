//! Drawing the Stackup page: the table of entries with the total against the target on
//! the left, a cross-section (not to scale) on the right, then the finish and colours
//! and the Fab Notes text.

use egui::{Color32, ComboBox, Rect, Role, Sense, Stroke, TextEdit, Ui, WidgetInfo, vec2};

use super::super::model::{Band, BoardSetupDraft, CrossBand, StackupDraft};
use super::no_pcb;
use crate::settings::{PageCx, field, problem_label};
use crate::tr;

const THICKNESS_WIDTH: f32 = 90.0;
const SECTION_WIDTH: f32 = 160.0;

/// The Stackup page.
pub fn stackup_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| table(ui, cx, board));
        ui.add_space(24.0);
        ui.vertical(|ui| cross_section(ui, board));
    });
    ui.add_space(12.0);
    looks(ui, board);
    ui.add_space(12.0);
    let stackup = board.stackup_mut();
    let label = tr!("board-setup-fab-notes");
    ui.strong(&label);
    let notes = ui.add(
        TextEdit::multiline(stackup.fab_notes_mut())
            .desired_rows(4)
            .desired_width(480.0),
    );
    notes.widget_info(|| WidgetInfo::labeled(Role::MultilineTextInput, true, &label));
    ui.weak(tr!("board-setup-fab-notes-note"));
}

/// Name, kind and thickness per entry, with the totals under it.
fn table(ui: &mut Ui, cx: &PageCx<'_>, board: &mut BoardSetupDraft) {
    let stackup = board.stackup_mut();
    let remainder = stackup.totals().and_then(|t| t.remainder);
    egui::Grid::new("board-setup-stackup")
        .num_columns(3)
        .striped(true)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.strong(tr!("board-setup-stackup-name"));
            ui.strong(tr!("board-setup-stackup-kind"));
            ui.strong(tr!("board-setup-stackup-thickness"));
            ui.end_row();
            for i in 0..stackup.rows().len() {
                let row = &stackup.rows()[i];
                let (name, copper) = (row.name.clone(), row.copper);
                ui.label(&name);
                ui.label(if copper {
                    tr!("board-setup-stackup-copper")
                } else {
                    tr!("board-setup-stackup-dielectric")
                });
                let label = tr!("board-setup-stackup-thickness-of", entry = name.as_str());
                let problem = cx.problem(&format!("stackup/row/{i}"));
                let hint = match remainder {
                    Some(rest) if !copper => super::mm(rest),
                    _ => String::new(),
                };
                if let Some(value) = stackup.row_mut(i) {
                    ui.vertical(|ui| {
                        let edit = TextEdit::singleline(value)
                            .hint_text(hint)
                            .desired_width(THICKNESS_WIDTH);
                        let response = ui.add(edit);
                        response.widget_info(|| {
                            WidgetInfo::labeled(Role::TextInput, true, label.clone())
                        });
                        if let Some(problem) = problem {
                            problem_label(ui, problem);
                        }
                    });
                }
                ui.end_row();
            }
            let label = tr!("board-setup-stackup-target");
            ui.strong(&label);
            ui.label("");
            let problem = cx.problem("stackup/thickness");
            field(ui, &label, stackup.target_mut(), THICKNESS_WIDTH, problem);
            ui.end_row();
        });
    ui.add_space(6.0);
    if let Some(total) = stackup.total_text() {
        ui.label(total);
    }
    if let Some(warning) = stackup.warning() {
        ui.colored_label(ui.visuals().warn_fg_color, warning);
    }
    ui.weak(tr!("board-setup-stackup-units"));
}

/// The colour a band is painted in; the mask follows the typed colour where it is a
/// common one.
fn band_color(band: Band, mask: &str, silk: &str) -> Color32 {
    let named = |name: &str| match name.trim().to_lowercase().as_str() {
        n if n.contains("green") => Some(Color32::from_rgb(0x2e, 0x7d, 0x32)),
        n if n.contains("red") => Some(Color32::from_rgb(0xb7, 0x1c, 0x1c)),
        n if n.contains("blue") => Some(Color32::from_rgb(0x15, 0x65, 0xc0)),
        n if n.contains("black") => Some(Color32::from_rgb(0x21, 0x21, 0x21)),
        n if n.contains("white") => Some(Color32::from_rgb(0xf5, 0xf5, 0xf5)),
        n if n.contains("yellow") => Some(Color32::from_rgb(0xf9, 0xa8, 0x25)),
        n if n.contains("purple") => Some(Color32::from_rgb(0x6a, 0x1b, 0x9a)),
        _ => None,
    };
    match band {
        Band::Silkscreen => named(silk).unwrap_or(Color32::from_rgb(0xf5, 0xf5, 0xf5)),
        Band::Mask => named(mask).unwrap_or(Color32::from_rgb(0x2e, 0x7d, 0x32)),
        Band::Copper => Color32::from_rgb(0xc8, 0x75, 0x33),
        Band::Dielectric => Color32::from_rgb(0xb5, 0xa6, 0x5a),
    }
}

/// How tall a band is drawn: by kind, not by thickness.
fn band_height(band: Band) -> f32 {
    match band {
        Band::Silkscreen => 5.0,
        Band::Mask => 7.0,
        Band::Copper => 10.0,
        Band::Dielectric => 34.0,
    }
}

/// The cross-section, not to scale, with each band's name and thickness beside it. It
/// is announced as an image listing the bands.
fn cross_section(ui: &mut Ui, board: &BoardSetupDraft) {
    let stackup = board.stackup();
    let bands = stackup.cross_section();
    ui.strong(tr!("board-setup-cross-section"));
    let height: f32 = bands.iter().map(|b| band_height(b.band)).sum();
    let width = SECTION_WIDTH + 170.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let mut y = rect.top();
    let text_color = ui.visuals().text_color();
    for b in &bands {
        let h = band_height(b.band);
        let band = Rect::from_min_size(egui::pos2(rect.left(), y), vec2(SECTION_WIDTH, h));
        let color = band_color(b.band, stackup.mask_color(), stackup.silk_color());
        painter.rect_filled(band, 0.0, color);
        painter.rect_stroke(
            band,
            0.0,
            Stroke::new(0.5, ui.visuals().weak_text_color()),
            egui::StrokeKind::Inside,
        );
        if matches!(b.band, Band::Copper | Band::Dielectric) {
            painter.text(
                egui::pos2(band.right() + 8.0, band.center().y),
                egui::Align2::LEFT_CENTER,
                band_text(b),
                egui::FontId::proportional(11.0),
                text_color,
            );
        }
        y += h;
    }
    let summary = bands
        .iter()
        .map(band_text)
        .collect::<Vec<_>>()
        .join(", ");
    let label = tr!("board-setup-cross-section-of", bands = summary);
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &label));
    ui.weak(tr!("board-setup-not-to-scale"));
}

fn band_text(b: &CrossBand) -> String {
    if b.thickness.is_empty() {
        b.name.clone()
    } else {
        format!("{} {} mm", b.name, b.thickness)
    }
}

/// The finish and the mask and silkscreen colours: each a drop-down of what the job's
/// Fab Preset suggests, with a text field for anything else.
fn looks(ui: &mut Ui, board: &mut BoardSetupDraft) {
    let suggestions = board.stackup().suggestions().clone();
    let stackup = board.stackup_mut();
    egui::Grid::new("board-setup-stackup-looks")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            let rows: [(String, &[String], fn(&mut StackupDraft) -> &mut String); 3] = [
                (
                    tr!("board-setup-finish"),
                    &suggestions.finish,
                    StackupDraft::finish_mut,
                ),
                (
                    tr!("board-setup-mask-color"),
                    &suggestions.mask_color,
                    StackupDraft::mask_color_mut,
                ),
                (
                    tr!("board-setup-silk-color"),
                    &suggestions.silk_color,
                    StackupDraft::silk_color_mut,
                ),
            ];
            for (label, options, value) in rows {
                ui.label(&label);
                let value = value(stackup);
                suggestion_box(ui, &label, options, value);
                ui.end_row();
            }
        });
}

/// A drop-down of `options` for `value`, which can also be typed.
fn suggestion_box(ui: &mut Ui, label: &str, options: &[String], value: &mut String) {
    ui.horizontal(|ui| {
        let shown = if value.trim().is_empty() {
            tr!("board-setup-unset")
        } else {
            value.clone()
        };
        let mut chosen = value.clone();
        let combo = ComboBox::from_id_salt(("board-setup-look", label))
            .selected_text(shown)
            .width(180.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut chosen, String::new(), tr!("board-setup-unset"));
                for option in options {
                    ui.selectable_value(&mut chosen, option.clone(), option);
                }
            });
        combo
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label));
        if chosen != *value {
            *value = chosen;
        }
        let typed = tr!("board-setup-typed", what = label);
        let edit = ui.add(TextEdit::singleline(value).desired_width(140.0));
        edit.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &typed));
    });
}
