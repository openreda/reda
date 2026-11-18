//! The bar, Command Palette and keys widgets, driven through AccessKit. How they look is
//! the gallery's job (`tests/widget_gallery`, the bars page).

use egui::accesskit::{Role, Toggled};
use egui::{Theme, Ui};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::theme::{self, TokensExt as _};
use reda_ui::widgets::{self, Glyph};

#[derive(Default)]
struct Page {
    clicked: Vec<&'static str>,
}

fn page(ui: &mut Ui, p: &mut Page) {
    let t = ui.tokens();
    widgets::bar_frame(&t).show(ui, |ui| {
        widgets::bar_row(ui, |ui| {
            let wire = ui.add(widgets::icon_button("wire").bar().on(true));
            reda_ui::a11y::name(&wire, egui::Role::Button, true, "Wire", Some(true), None);
            widgets::bar_separator(ui);
            let funnel = ui.add(widgets::glyph_button(Glyph::Funnel));
            reda_ui::a11y::name(&funnel, egui::Role::Button, true, "Funnel", None, None);
            if funnel.clicked() {
                p.clicked.push("funnel");
            }
        });
    });
    widgets::rows(ui, |ui| {
        let row = widgets::palette_row("Save All")
            .icon("save_all")
            .note("File › Save All")
            .shortcut("Ctrl+S")
            .note("recently used")
            .selected(true);
        if ui.add(row).clicked() {
            p.clicked.push("save all");
        }
        let unavailable = widgets::palette_row("Redo")
            .icon("redo")
            .note("Nothing to redo");
        if ui.add_enabled(false, unavailable).clicked() {
            p.clicked.push("redo");
        }
        if ui.add(widgets::palette_row("Sheet 1")).clicked() {
            p.clicked.push("sheet");
        }
    });
    let rows = [("Shift+W".to_owned(), "Select Width".to_owned())];
    widgets::key_list(ui, "keys", &rows);
}

fn harness() -> Harness<'static, Page> {
    let mut installed = false;
    let mut h = Harness::builder().with_size([600.0, 400.0]).build_ui_state(
        move |ui, p: &mut Page| {
            if !installed {
                theme::install(ui.ctx(), Theme::Light.into());
                installed = true;
            }
            page(ui, p);
        },
        Page::default(),
    );
    h.run();
    h
}

#[test]
fn bar_buttons_are_the_control_height_and_two_points_apart() {
    let h = harness();
    let side = h.ctx.tokens().control_height();
    let wire = h.get_by_role_and_label(Role::Button, "Wire").rect();
    let funnel = h.get_by_role_and_label(Role::Button, "Funnel").rect();
    assert_eq!(wire.size(), egui::vec2(side, side));
    assert_eq!(funnel.size(), egui::vec2(side, side));
    // The separator between them, with a bar gap on each side.
    let between = funnel.left() - wire.right();
    assert_eq!(between, widgets::BAR_SEPARATOR + 2.0 * widgets::BAR_GAP);
}

#[test]
fn a_palette_row_is_named_by_its_title_then_its_notes_and_key_in_order() {
    let h = harness();
    let row = h.get_by_role_and_label(
        Role::Button,
        "Save All File › Save All  ·  Ctrl+S  ·  recently used",
    );
    assert_eq!(row.accesskit_node().toggled(), Some(Toggled::True));
    assert_eq!(row.rect().height(), h.ctx.tokens().row_height());
    let plain = h.get_by_role_and_label(Role::Button, "Sheet 1");
    assert_eq!(plain.accesskit_node().toggled(), Some(Toggled::False));
}

#[test]
fn a_palette_row_runs_on_a_click_unless_it_is_unavailable() {
    let mut h = harness();
    h.get_by_label_contains("Save All File").click();
    h.run();
    let redo = h.get_by_label("Redo Nothing to redo");
    assert!(redo.accesskit_node().is_disabled());
    redo.click();
    h.run();
    assert_eq!(h.state().clicked, ["save all"]);
}

#[test]
fn the_key_list_shows_each_key_and_its_action() {
    let h = harness();
    h.get_by_role_and_label(Role::Label, "Shift+W");
    h.get_by_role_and_label(Role::Label, "Select Width");
}
