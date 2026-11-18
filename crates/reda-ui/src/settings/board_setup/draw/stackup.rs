//! Drawing the Stackup page: the table of entries with the total against the target on
//! the left, a cross-section (not to scale) on the right, then the finish and colours
//! and the Fab Notes text.

use egui::{Color32, Rect, Role, Sense, Stroke, Ui, WidgetInfo, vec2};

use super::super::model::{Band, BoardSetupDraft, CrossBand, StackupDraft};
use super::no_pcb;
use crate::settings::{PageCx, field, layout, problem_label, strong};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column};

const THICKNESS_WIDTH: f32 = 90.0;

/// Where one of the Stackup's texts lives in the drafts.
type TextOf = fn(&mut StackupDraft) -> &mut String;
/// The bands of the cross-section.
const SECTION_WIDTH: f32 = 160.0;
/// Room beside the bands for each one's name and thickness.
const SECTION_LABELS: f32 = 170.0;
/// The table's column, which the totals and notes under it wrap to.
const TABLE_WIDTH: f32 = 360.0;
/// The Fab Notes text.
const NOTES_WIDTH: f32 = 480.0;
/// How many lines the Fab Notes field shows.
const NOTES_ROWS: usize = 4;
/// A finish or colour's dropdown of suggestions.
const SUGGESTION_WIDTH: f32 = 180.0;
/// A finish or colour typed by hand.
const TYPED_WIDTH: f32 = 140.0;

/// The Stackup page.
pub fn stackup_page(ui: &mut Ui, cx: &mut PageCx<'_>, board: &mut BoardSetupDraft) {
    if no_pcb(ui, board) {
        return;
    }
    let t = ui.tokens();
    // The cross-section sits beside the table when the page is wide enough, else under
    // it, so its labels are never cut by the page's edge.
    let beside = t.size.of(TABLE_WIDTH + SECTION_WIDTH + SECTION_LABELS) + t.space.dialog;
    if ui.available_width() >= beside {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_max_width(t.size.of(TABLE_WIDTH));
                table(ui, cx, board);
            });
            ui.add_space(t.space.dialog);
            ui.vertical(|ui| cross_section(ui, board));
        });
    } else {
        ui.vertical(|ui| {
            ui.set_max_width(t.size.of(TABLE_WIDTH));
            table(ui, cx, board);
        });
        ui.add_space(t.space.section);
        cross_section(ui, board);
    }
    ui.add_space(t.space.section);
    looks(ui, board);
    ui.add_space(t.space.section);
    let stackup = board.stackup_mut();
    let label = tr!("board-setup-fab-notes");
    strong(ui, &label);
    let notes = ui.add(
        widgets::text_field(stackup.fab_notes_mut())
            .rows(NOTES_ROWS)
            .width(t.size.of(NOTES_WIDTH)),
    );
    notes.widget_info(|| WidgetInfo::labeled(Role::MultilineTextInput, true, &label));
    layout::muted(ui, tr!("board-setup-fab-notes-note"));
}

/// Name, kind and thickness per entry, with the totals under it.
fn table(ui: &mut Ui, cx: &PageCx<'_>, board: &mut BoardSetupDraft) {
    let stackup = board.stackup_mut();
    let remainder = stackup.totals().and_then(|t| t.remainder);
    let t = ui.tokens();
    widgets::table("board-setup-stackup")
        .columns([Column::auto(), Column::auto(), Column::auto()])
        .show(ui, |table| {
            table.header(|h| {
                h.text(tr!("board-setup-stackup-name"));
                h.text(tr!("board-setup-stackup-kind"));
                h.text(tr!("board-setup-stackup-thickness"));
            });
            for i in 0..stackup.rows().len() {
                let row = &stackup.rows()[i];
                let (name, copper) = (row.name.clone(), row.copper);
                let label = tr!("board-setup-stackup-thickness-of", entry = name.as_str());
                let problem = cx.problem(&format!("stackup/row/{i}"));
                let hint = match remainder {
                    Some(rest) if !copper => super::mm(rest),
                    _ => String::new(),
                };
                table.row(|row| {
                    row.text(&name);
                    row.text(if copper {
                        tr!("board-setup-stackup-copper")
                    } else {
                        tr!("board-setup-stackup-dielectric")
                    });
                    row.ui(|ui| {
                        if let Some(value) = stackup.row_mut(i) {
                            ui.vertical(|ui| {
                                let edit = widgets::text_field(value)
                                    .hint(hint)
                                    .width(t.size.of(THICKNESS_WIDTH))
                                    .invalid(problem.is_some());
                                let response = ui.add(edit);
                                response.widget_info(|| {
                                    WidgetInfo::labeled(Role::TextInput, true, label.clone())
                                });
                                if let Some(problem) = problem {
                                    problem_label(ui, problem);
                                }
                            });
                        }
                    });
                });
            }
            let label = tr!("board-setup-stackup-target");
            table.row(|row| {
                row.ui(|ui| strong(ui, &label));
                row.empty();
                let problem = cx.problem("stackup/thickness");
                row.ui(|ui| field(ui, &label, stackup.target_mut(), THICKNESS_WIDTH, problem));
            });
        });
    ui.add_space(t.space.stack);
    if let Some(total) = stackup.total_text() {
        layout::text(ui, total);
    }
    if let Some(warning) = stackup.warning() {
        layout::warning(ui, warning);
    }
    layout::muted(ui, tr!("board-setup-stackup-units"));
}

/// The colour a band is painted in, as red, green and blue; the mask and silkscreen
/// follow the typed colour where it is a common one. These are the board's own colours
/// (data, like a Layer's), not the theme's, so they stay the same in light and dark.
fn band_rgb(band: Band, mask: &str, silk: &str) -> (u8, u8, u8) {
    let named = |name: &str| match name.trim().to_lowercase().as_str() {
        n if n.contains("green") => Some((0x2e, 0x7d, 0x32)),
        n if n.contains("red") => Some((0xb7, 0x1c, 0x1c)),
        n if n.contains("blue") => Some((0x15, 0x65, 0xc0)),
        n if n.contains("black") => Some((0x21, 0x21, 0x21)),
        n if n.contains("white") => Some((0xf5, 0xf5, 0xf5)),
        n if n.contains("yellow") => Some((0xf9, 0xa8, 0x25)),
        n if n.contains("purple") => Some((0x6a, 0x1b, 0x9a)),
        _ => None,
    };
    match band {
        Band::Silkscreen => named(silk).unwrap_or((0xf5, 0xf5, 0xf5)),
        Band::Mask => named(mask).unwrap_or((0x2e, 0x7d, 0x32)),
        Band::Copper => (0xc8, 0x75, 0x33),
        Band::Dielectric => (0xb5, 0xa6, 0x5a),
    }
}

/// [`band_rgb`] as a colour to paint with.
fn band_color(band: Band, mask: &str, silk: &str) -> Color32 {
    let (r, g, b) = band_rgb(band, mask, silk);
    // The board's own mask, silkscreen and material colours: data, not theme values.
    crate::widgets::data_color(r, g, b)
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
    let t = ui.tokens();
    strong(ui, tr!("board-setup-cross-section"));
    let height: f32 = bands.iter().map(|b| band_height(b.band)).sum();
    let width = SECTION_WIDTH + t.size.of(SECTION_LABELS);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter_at(rect);
    let mut y = rect.top();
    let text_color = t.color.text.primary;
    for b in &bands {
        let h = band_height(b.band);
        let band = Rect::from_min_size(egui::pos2(rect.left(), y), vec2(SECTION_WIDTH, h));
        let color = band_color(b.band, stackup.mask_color(), stackup.silk_color());
        painter.rect_filled(band, 0.0, color);
        painter.rect_stroke(
            band,
            0.0,
            Stroke::new(t.stroke.hairline, t.color.stroke.strong),
            egui::StrokeKind::Inside,
        );
        if matches!(b.band, Band::Copper | Band::Dielectric) {
            painter.text(
                egui::pos2(band.right() + t.space.gap, band.center().y),
                egui::Align2::LEFT_CENTER,
                band_text(b),
                t.ty.small.font_id(),
                text_color,
            );
        }
        y += h;
    }
    let summary = bands.iter().map(band_text).collect::<Vec<_>>().join(", ");
    let label = tr!("board-setup-cross-section-of", bands = summary);
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &label));
    layout::muted(ui, tr!("board-setup-not-to-scale"));
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
    let rows: [(String, &[String], TextOf); 3] = [
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
    widgets::form("board-setup-stackup-looks").show(ui, |ui| {
        for (label, options, value) in rows {
            let value = value(stackup);
            widgets::form_row(ui, &label, |ui| suggestion_box(ui, &label, options, value));
        }
    });
}

/// A drop-down of `options` for `value`, which can also be typed.
fn suggestion_box(ui: &mut Ui, label: &str, options: &[String], value: &mut String) {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        let shown = if value.trim().is_empty() {
            tr!("board-setup-unset")
        } else {
            value.clone()
        };
        let mut chosen = value.clone();
        let combo = widgets::dropdown(("board-setup-look", label), shown)
            .width(t.size.of(SUGGESTION_WIDTH))
            .show(ui, |ui| {
                let unset = tr!("board-setup-unset");
                widgets::choice_value(ui, &mut chosen, String::new(), unset);
                for option in options {
                    widgets::choice_value(ui, &mut chosen, option.clone(), option);
                }
            });
        combo
            .response
            .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label));
        if chosen != *value {
            *value = chosen;
        }
        let typed = tr!("board-setup-typed", what = label);
        let edit = ui.add(widgets::text_field(value).width(t.size.of(TYPED_WIDTH)));
        edit.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &typed));
    });
}
