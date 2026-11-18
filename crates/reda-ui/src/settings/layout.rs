//! The layout every settings window shares
//! (docs/spec/v0.1/design-language.md#dialogs-and-settings-windows): an optional bar
//! across the top, a sidebar on the base colour holding the page tree, the page on the
//! floating surface, and a row along the bottom with the window's buttons at its trailing
//! end. Project Settings, Board Setup, the Output Job window and the Change List fill it
//! from [`super::SettingsDialog`], Preferences from its own dialog.
//!
//! The pieces of a page that every window draws the same way are here too: the page
//! heading with its buttons ([`page_header`]), a headed group of settings ([`group`]) and
//! the plain texts of a page ([`muted`], [`error`], [`warning`]).

use egui::{Align, Frame, Layout, Role, ScrollArea, Sense, Ui, WidgetInfo, pos2, vec2};

use crate::theme::{Tokens, TokensExt as _};
use crate::widgets;

/// The width of the sidebar's page tree.
pub const SIDEBAR_WIDTH: f32 = 200.0;

/// One page in a settings window's page tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TreeRow {
    /// The page's title.
    pub title: String,
    /// The row's accessible name, when the title alone does not say enough (a page with
    /// changed settings, a page with an invalid value).
    pub name: Option<String>,
    /// The heading of the group this row starts, drawn above it.
    pub heading: Option<String>,
    /// How many levels the row is indented.
    pub depth: usize,
    /// The page shown.
    pub selected: bool,
    /// Dimmed: a page the search found nothing on.
    pub muted: bool,
    /// The page holds an invalid value.
    pub invalid: bool,
    /// Small text at the row's trailing end: the count of changed settings.
    pub trailing: Option<String>,
}

/// What a settings window puts in the shared layout.
pub trait Content {
    /// Whether the window has a bar across its top.
    fn has_top_bar(&self) -> bool {
        false
    }

    /// Draws the bar across the top (Preferences' search), in one row.
    fn top_bar(&mut self, _ui: &mut Ui) {}

    /// The page tree's rows, in order. A window with none has no sidebar.
    fn tree(&mut self) -> Vec<TreeRow>;

    /// The tree row at `index` was clicked.
    fn choose(&mut self, index: usize);

    /// Draws the shown page, inside the page's scroll area.
    fn page(&mut self, ui: &mut Ui);

    /// Draws what sits above the bottom row: the error of the last Apply.
    fn notice(&mut self, _ui: &mut Ui) {}

    /// Draws the leading end of the bottom row, left to right.
    fn footer(&mut self, _ui: &mut Ui) {}

    /// Draws the window's buttons at the trailing end of the bottom row. The row runs
    /// from the trailing edge, so add the last button first.
    fn buttons(&mut self, ui: &mut Ui);
}

/// Draws a settings window's content into `ui` (its tab).
pub fn show(ui: &mut Ui, content: &mut dyn Content) {
    let t = ui.tokens();
    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
        ui.horizontal(|ui| {
            content.footer(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                content.buttons(ui);
            });
        });
        content.notice(ui);
        ui.add_space(t.space.stack);
        ui.with_layout(Layout::top_down(Align::Min), |ui| {
            if content.has_top_bar() {
                ui.horizontal(|ui| content.top_bar(ui));
                ui.add_space(t.space.stack);
            }
            ui.horizontal_top(|ui| {
                let rows = content.tree();
                if !rows.is_empty()
                    && let Some(chosen) = sidebar(ui, &t, &rows)
                {
                    content.choose(chosen);
                }
                ui.vertical(|ui| {
                    Frame::new()
                        .fill(t.color.surface.floating)
                        .corner_radius(t.radius.card)
                        .inner_margin(t.space.panel)
                        .show(ui, |ui| {
                            ScrollArea::both()
                                .id_salt("settings-page")
                                .auto_shrink(false)
                                .show(ui, |ui| content.page(ui));
                        });
                });
            });
        });
    });
}

/// The sidebar: the page tree on the base colour. Returns the row that was clicked.
fn sidebar(ui: &mut Ui, t: &Tokens, rows: &[TreeRow]) -> Option<usize> {
    let mut chosen = None;
    let size = vec2(SIDEBAR_WIDTH + 2.0 * t.space.menu, ui.available_height());
    ui.allocate_ui_with_layout(size, Layout::top_down(Align::Min), |ui| {
        Frame::new()
            .fill(t.color.surface.base)
            .corner_radius(t.radius.card)
            .inner_margin(t.space.menu)
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt("settings-tree")
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        widgets::rows(ui, |ui| {
                            for (i, row) in rows.iter().enumerate() {
                                if tree_row(ui, t, row).clicked() {
                                    chosen = Some(i);
                                }
                            }
                        });
                    });
            });
    });
    chosen
}

/// One row of the page tree, under its group's heading when it starts one.
fn tree_row(ui: &mut Ui, t: &Tokens, row: &TreeRow) -> egui::Response {
    let size = vec2(ui.available_width(), t.row_height());
    if let Some(heading) = &row.heading {
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let ink = t.color.text.muted;
        let galley = ui
            .painter()
            .layout_no_wrap(heading.clone(), t.ty.small.font_id(), ink);
        let at = pos2(
            rect.left() + t.space.row_x,
            rect.center().y - galley.size().y / 2.0,
        );
        ui.painter().galley(at.round(), galley, ink);
    }
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let indented = rect.with_min_x(rect.left() + widgets::TREE_INDENT * row.depth as f32);
    let mut line = widgets::list_row(row.title.as_str())
        .selected(row.selected)
        .muted(row.muted);
    if row.invalid {
        line = line.status(widgets::Status::Error);
    }
    if let Some(trailing) = &row.trailing {
        line = line.trailing(trailing.as_str());
    }
    let response = ui.put(indented, line);
    let name = row.name.clone().unwrap_or_else(|| row.title.clone());
    response.widget_info(|| WidgetInfo::labeled(Role::TreeItem, true, name.clone()));
    response
}

/// A page's heading, with whatever `trailing` adds at its trailing end (Reset page). The
/// trailing end runs from the edge, so add the last button first.
pub fn page_header(ui: &mut Ui, title: &str, trailing: impl FnOnce(&mut Ui)) {
    let t = ui.tokens();
    ui.horizontal(|ui| {
        ui.label(t.ty.heading.rich(title).color(t.color.text.primary));
        ui.with_layout(Layout::right_to_left(Align::Center), trailing);
    });
    ui.add_space(t.space.stack);
}

/// The heading of a group of settings on a page, a `section` below what is above it.
pub fn group(ui: &mut Ui, heading: &str) {
    let t = ui.tokens();
    ui.add_space(t.space.section);
    ui.label(t.ty.title.rich(heading).color(t.color.text.primary));
}

/// Body text in the primary colour.
pub fn text(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.text.primary))
}

/// Secondary text: an empty list's note, a caption.
pub fn muted(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.text.muted))
}

/// A caption: one line of small muted text under a setting, `indent` in from the page's
/// edge so it starts where the setting's label does.
pub fn caption(ui: &mut Ui, indent: f32, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    let text = t.ty.small.rich(text).color(t.color.text.muted);
    indented(ui, indent, text)
}

/// A caption that warns: the setting is not doing what it says.
pub fn caption_warning(ui: &mut Ui, indent: f32, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    let text = t.ty.small.rich(text).color(t.color.warning.text);
    indented(ui, indent, text)
}

fn indented(ui: &mut Ui, indent: f32, text: egui::RichText) -> egui::Response {
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.add(egui::Label::new(text).extend())
    })
    .inner
}

/// Something that went wrong, in the error tone.
pub fn error(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.error.text))
}

/// Something to look at, in the warning tone.
pub fn warning(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    let t = ui.tokens();
    ui.label(t.ty.body.rich(text).color(t.color.warning.text))
}
