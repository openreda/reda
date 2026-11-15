//! The REDA widgets, driven through AccessKit: what each control does when used, and the
//! heights the two Densities give them. How they look is the gallery's job
//! (`tests/widget_gallery`).

use egui::accesskit::Role;
use egui::{Theme, Ui};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::theme::{self, Density, Settings, TokensExt as _};
use reda_ui::widgets::{self, Choice};

/// What the form below edits.
#[derive(Default)]
struct Form {
    runs: u32,
    snap: bool,
    visible: bool,
    animate: bool,
    metric: bool,
    name: String,
    layer: u8,
    theme: u8,
    size: i64,
    font: usize,
    opacity: f64,
    links: u32,
}

const FONTS: [&str; 4] = ["Default (Inter)", "DejaVu Sans", "Noto Sans", "Noto Serif"];

fn form(ui: &mut Ui, f: &mut Form) {
    if ui.add(widgets::button("Run").primary()).clicked() {
        f.runs += 1;
    }
    ui.add_enabled(false, widgets::button("Stop").danger());
    if ui.add(widgets::button("Snap").on(f.snap)).clicked() {
        f.snap = !f.snap;
    }
    ui.add(widgets::checkbox(&mut f.visible, "Visible"));
    ui.add(widgets::switch(&mut f.animate, "Animate"));
    if ui.add(widgets::radio(f.metric, "Metric")).clicked() {
        f.metric = true;
    }
    if ui.add(widgets::link("Open the guide")).clicked() {
        f.links += 1;
    }
    let name = ui.add(widgets::text_field(&mut f.name).hint("Net name"));
    name.widget_info(|| egui::WidgetInfo::labeled(Role::TextInput, true, "Name"));
    let layer_name = ["Top", "Inner", "Bottom"][usize::from(f.layer)];
    let layer = widgets::dropdown("layer", layer_name).show(ui, |ui| {
        widgets::choice_value(ui, &mut f.layer, 0, "Top");
        widgets::choice_value(ui, &mut f.layer, 1, "Inner");
        widgets::choice_value(ui, &mut f.layer, 2, "Bottom");
    });
    layer
        .response
        .widget_info(|| egui::WidgetInfo::labeled(Role::ComboBox, true, "Layer"));
    let themes = [(0u8, "System"), (1, "Light"), (2, "Dark")];
    ui.add(widgets::segmented(&mut f.theme, &themes));
    let size = ui.add(widgets::stepper_field(&mut f.size, 11..=16).suffix("px"));
    size.widget_info(|| egui::WidgetInfo::labeled(Role::TextInput, true, "Text size"));
    let fonts = FONTS.map(Choice::new);
    let font = widgets::filter_dropdown("font", FONTS[f.font])
        .hint("Filter")
        .none_text("No fonts match")
        .show(ui, &fonts, Some(f.font));
    font.response
        .widget_info(|| egui::WidgetInfo::labeled(Role::ComboBox, true, "UI font"));
    if let Some(index) = font.index {
        f.font = index;
    }
    ui.add(
        widgets::slider(&mut f.opacity, 0.0..=10.0)
            .step(1.0)
            .text("Opacity"),
    );
    ui.add(widgets::progress(0.5).named("Export"));
}

fn harness(density: Density) -> Harness<'static, Form> {
    let mut installed = false;
    let form_state = Form {
        size: 13,
        opacity: 4.0,
        ..Form::default()
    };
    Harness::builder().with_size([420.0, 640.0]).build_ui_state(
        move |ui, f: &mut Form| {
            if !installed {
                theme::install(ui.ctx(), Theme::Light.into());
                theme::set(
                    ui.ctx(),
                    &Settings {
                        density,
                        ..Settings::default()
                    },
                );
                installed = true;
            }
            form(ui, f);
        },
        form_state,
    )
}

/// A press and release at `at`.
fn click_at(h: &Harness<'_, Form>, at: egui::Pos2) {
    h.drag_at(at);
    h.drop_at(at);
}

fn comfortable() -> Harness<'static, Form> {
    let mut h = harness(Density::Comfortable);
    h.run();
    h
}

#[test]
fn a_button_runs_when_clicked_and_not_when_disabled() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::Button, "Run").click();
    h.run();
    assert_eq!(h.state().runs, 1);
    assert!(h.get_by_label("Stop").accesskit_node().is_disabled());
}

#[test]
fn a_focused_button_runs_on_enter() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::Button, "Run").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().runs, 1);
}

#[test]
fn a_toggle_says_whether_it_is_on() {
    let mut h = comfortable();
    let snap = h.get_by_role_and_label(Role::Button, "Snap");
    assert_eq!(
        snap.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    snap.click();
    h.run();
    assert!(h.state().snap);
    let snap = h.get_by_role_and_label(Role::Button, "Snap");
    assert_eq!(
        snap.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
}

#[test]
fn a_check_box_a_switch_and_a_radio_flip_when_clicked() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::CheckBox, "Visible").click();
    h.get_by_role_and_label(Role::Switch, "Animate").click();
    h.get_by_role_and_label(Role::RadioButton, "Metric").click();
    h.run();
    let f = h.state();
    assert!(f.visible && f.animate && f.metric);
    h.get_by_role_and_label(Role::CheckBox, "Visible").click();
    h.run();
    assert!(!h.state().visible);
}

#[test]
fn a_link_is_a_link_and_is_followed_when_clicked() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::Link, "Open the guide")
        .click();
    h.run();
    assert_eq!(h.state().links, 1);
}

#[test]
fn a_text_field_takes_what_is_typed() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::TextInput, "Name").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Name")
        .type_text("GND");
    h.run();
    assert_eq!(h.state().name, "GND");
}

#[test]
fn a_dropdown_opens_its_list_and_takes_the_row_clicked() {
    let mut h = comfortable();
    assert!(h.query_by_label("Bottom").is_none());
    h.get_by_role_and_label(Role::ComboBox, "Layer").click();
    h.run();
    h.get_by_label("Bottom").click();
    h.run();
    assert_eq!(h.state().layer, 2);
    assert!(h.query_by_label("Inner").is_none(), "the list closed");
}

#[test]
fn a_segmented_control_takes_the_segment_clicked() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::RadioButton, "Dark").click();
    h.run();
    assert_eq!(h.state().theme, 2);
    let dark = h.get_by_role_and_label(Role::RadioButton, "Dark");
    assert_eq!(
        dark.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
}

#[test]
fn a_stepper_field_steps_with_the_arrows_and_stops_at_its_ends() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::TextInput, "Text size")
        .focus();
    h.run();
    h.key_press(egui::Key::ArrowUp);
    h.run();
    assert_eq!(h.state().size, 14);
    for _ in 0..5 {
        h.key_press(egui::Key::ArrowUp);
        h.run();
    }
    assert_eq!(h.state().size, 16);
    for _ in 0..9 {
        h.key_press(egui::Key::ArrowDown);
        h.run();
    }
    assert_eq!(h.state().size, 11);
}

#[test]
fn a_stepper_field_steps_when_its_halves_are_clicked() {
    let mut h = comfortable();
    let rect = h.get_by_role_and_label(Role::TextInput, "Text size").rect();
    // The halves are in the column at the field's trailing end, right of the text.
    let column = rect.right() + 30.0;
    let (up, down) = (
        egui::pos2(column, rect.center().y - 7.0),
        egui::pos2(column, rect.center().y + 7.0),
    );
    click_at(&h, up);
    h.run();
    assert_eq!(h.state().size, 14);
    click_at(&h, down);
    h.run();
    click_at(&h, down);
    h.run();
    assert_eq!(h.state().size, 12);
}

#[test]
fn a_stepper_field_takes_a_typed_number_on_enter_and_keeps_it_in_range() {
    let mut h = comfortable();
    let field = h.get_by_role_and_label(Role::TextInput, "Text size");
    field.focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Text size")
        .type_text("15");
    h.run();
    assert_eq!(h.state().size, 13, "nothing applies while typing");
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().size, 15);

    h.get_by_role_and_label(Role::TextInput, "Text size")
        .focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Text size")
        .type_text("40");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().size, 16, "clamped to the top of the range");

    h.get_by_role_and_label(Role::TextInput, "Text size")
        .focus();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Text size")
        .type_text("big");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().size, 16, "text that is no number changes nothing");
}

#[test]
fn a_filterable_dropdown_narrows_as_one_types_and_enter_picks_the_lit_row() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::ComboBox, "UI font").click();
    h.run();
    assert!(h.query_by_label("DejaVu Sans").is_some());
    // The search field has the keyboard as soon as the list opens.
    let filter = h.get_by_role_and_label(Role::SearchInput, "Filter");
    assert!(filter.accesskit_node().is_focused());
    filter.type_text("noto");
    h.run();
    assert!(h.query_by_label("DejaVu Sans").is_none());
    assert!(h.query_by_label("Noto Sans").is_some());
    h.key_press(egui::Key::ArrowDown);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(FONTS[h.state().font], "Noto Serif");
    assert!(h.query_by_label("Noto Sans").is_none(), "the list closed");
}

#[test]
fn a_filterable_dropdown_says_when_nothing_matches_and_takes_a_click() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::ComboBox, "UI font").click();
    h.run();
    h.get_by_role_and_label(Role::SearchInput, "Filter")
        .type_text("zz");
    h.run();
    for font in &FONTS[1..] {
        assert!(h.query_by_label(font).is_none());
    }
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().font, 0, "Enter with no match picks nothing");
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(
        h.query_by_role_and_label(Role::SearchInput, "Filter")
            .is_none()
    );

    h.get_by_role_and_label(Role::ComboBox, "UI font").click();
    h.run();
    h.get_by_label("DejaVu Sans").click();
    h.run();
    assert_eq!(FONTS[h.state().font], "DejaVu Sans");
}

#[test]
fn a_slider_takes_a_value_from_a_screen_reader_and_steps_with_the_arrows() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::Slider, "Opacity")
        .set_value_accesskit(7.0);
    h.run();
    assert_eq!(h.state().opacity, 7.0);
    h.get_by_role_and_label(Role::Slider, "Opacity").focus();
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert_eq!(h.state().opacity, 8.0);
    h.get_by_role_and_label(Role::Slider, "Opacity")
        .set_value_accesskit(99.0);
    h.run();
    assert_eq!(h.state().opacity, 10.0, "kept in range");
}

#[test]
fn a_slider_follows_the_pointer_along_its_rail() {
    let mut h = comfortable();
    let rail = h.get_by_role_and_label(Role::Slider, "Opacity").rect();
    click_at(&h, egui::pos2(rail.right() - 1.0, rail.center().y));
    h.run();
    assert_eq!(h.state().opacity, 10.0);
    click_at(&h, egui::pos2(rail.left() + 1.0, rail.center().y));
    h.run();
    assert_eq!(h.state().opacity, 0.0);
}

/// The heights of every control that is a control tall.
fn control_heights(h: &Harness<'_, Form>) -> Vec<f32> {
    let mut heights = vec![
        h.get_by_role_and_label(Role::Button, "Run").rect().height(),
        h.get_by_role_and_label(Role::Button, "Snap")
            .rect()
            .height(),
        h.get_by_role_and_label(Role::CheckBox, "Visible")
            .rect()
            .height(),
        h.get_by_role_and_label(Role::Switch, "Animate")
            .rect()
            .height(),
        h.get_by_role_and_label(Role::ComboBox, "Layer")
            .rect()
            .height(),
        h.get_by_role_and_label(Role::ComboBox, "UI font")
            .rect()
            .height(),
        h.get_by_role_and_label(Role::Slider, "Opacity")
            .rect()
            .height(),
    ];
    // A segment sits 2 px inside its track on each side.
    heights.push(
        h.get_by_role_and_label(Role::RadioButton, "Dark")
            .rect()
            .height()
            + 4.0,
    );
    heights
}

#[test]
fn controls_are_30_px_tall_in_comfortable_and_26_in_compact_at_13_px_text() {
    for (density, height) in [(Density::Comfortable, 30.0), (Density::Compact, 26.0)] {
        let mut h = harness(density);
        h.run();
        for found in control_heights(&h) {
            assert_eq!(found, height, "{density:?}");
        }
    }
}

#[test]
fn a_control_is_as_tall_as_its_line_plus_its_padding_at_every_text_size() {
    for size in Settings::TEXT_SIZES {
        let ctx = egui::Context::default();
        theme::set(
            &ctx,
            &Settings {
                text_size: size,
                ..Settings::default()
            },
        );
        let t = ctx.tokens();
        assert_eq!(t.control_height(), t.ty.body.line + 2.0 * t.space.control_y);
    }
}
