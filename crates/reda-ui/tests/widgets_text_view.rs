//! The text view, driven through AccessKit: a long text that can be read and selected but
//! not changed. How it looks is the gallery's job (`tests/widget_gallery/text_view.rs`).

use egui::accesskit::Role;
use egui::{Theme, WidgetInfo};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::theme;
use reda_ui::widgets;

const TEXT: &str = "MIT License\n\nCopyright (c) the egui authors";

fn harness() -> Harness<'static> {
    Harness::builder().with_size([400.0, 240.0]).build_ui(|ui| {
        theme::install(ui.ctx(), Theme::Light.into());
        let view = ui.add(widgets::text_view(TEXT).mono());
        view.widget_info(|| WidgetInfo::labeled(Role::MultilineTextInput, true, "Notices"));
    })
}

#[test]
fn a_text_view_reads_out_its_whole_text() {
    let mut h = harness();
    h.run();
    let view = h.get_by_role_and_label(Role::MultilineTextInput, "Notices");
    assert_eq!(view.accesskit_node().value().unwrap_or_default(), TEXT);
}

#[test]
fn typing_into_a_text_view_changes_nothing() {
    let mut h = harness();
    h.run();
    h.get_by_role_and_label(Role::MultilineTextInput, "Notices")
        .click();
    h.run();
    h.get_by_role_and_label(Role::MultilineTextInput, "Notices")
        .type_text("typed");
    h.run();
    let view = h.get_by_role_and_label(Role::MultilineTextInput, "Notices");
    assert_eq!(view.accesskit_node().value().unwrap_or_default(), TEXT);
}

#[test]
fn a_text_view_grows_to_its_whole_text() {
    let mut h = harness();
    h.run();
    let view = h.get_by_role_and_label(Role::MultilineTextInput, "Notices");
    // Three lines of the mono style, none of them cut off.
    let line = theme::Tokens::new(Theme::Light, &theme::Settings::default())
        .ty
        .mono
        .line;
    assert!(
        view.rect().height() >= 3.0 * line - 0.5,
        "{:?}",
        view.rect()
    );
}
