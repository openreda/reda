//! Badges, count pills and the colour button, driven through AccessKit. How they look is
//! the gallery's job (`tests/widget_gallery`).

use egui::accesskit::{Role, Toggled};
use egui::{Color32, Theme, Ui};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::theme::{self, Density, Settings};
use reda_ui::widgets::{self, BadgeTone, Status};

/// What the page below shows and edits.
struct Page {
    errors_shown: bool,
    colour: Color32,
    recolours: u32,
}

fn page(ui: &mut Ui, p: &mut Page) {
    ui.add(widgets::badge("Locally edited").tone(BadgeTone::Warning));
    ui.add(widgets::badge("Clean").status(Status::Success));
    ui.add(widgets::count_pill(12));
    ui.add(widgets::count_pill(3));
    let chip = widgets::badge("3 Errors").status(Status::Error);
    if ui.add(chip.filter(p.errors_shown)).clicked() {
        p.errors_shown = !p.errors_shown;
    }
    if ui
        .add(widgets::color_button(&mut p.colour, "Colour of Top Copper"))
        .changed()
    {
        p.recolours += 1;
    }
    ui.add(widgets::swatch_none().button("No colour"));
}

fn harness(density: Density) -> Harness<'static, Page> {
    let mut installed = false;
    let state = Page {
        errors_shown: true,
        colour: widgets::data_color(0xc8, 0x34, 0x34),
        recolours: 0,
    };
    let mut h = Harness::builder().with_size([520.0, 640.0]).build_ui_state(
        move |ui, p: &mut Page| {
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
            page(ui, p);
        },
        state,
    );
    h.run();
    h
}

#[test]
fn a_badge_is_a_label_of_its_text() {
    let h = harness(Density::Comfortable);
    h.get_by_role_and_label(Role::Label, "Locally edited");
    h.get_by_role_and_label(Role::Label, "Clean");
}

#[test]
fn a_badge_is_the_small_line_plus_its_padding_in_both_densities() {
    for density in [Density::Comfortable, Density::Compact] {
        let h = harness(density);
        let t = theme::TokensExt::tokens(&h.ctx);
        let height = t.ty.small.line + 2.0 * widgets::BADGE_PAD_Y;
        for label in ["Locally edited", "Clean", "12", "3 Errors"] {
            assert_eq!(h.get_by_label(label).rect().height(), height, "{label}");
        }
    }
}

#[test]
fn a_count_pill_grows_with_its_number_and_is_never_narrower_than_tall() {
    let h = harness(Density::Comfortable);
    let one = h.get_by_label("3").rect();
    assert!(one.width() >= one.height());
    let two = h.get_by_label("12").rect();
    assert!(two.width() > one.width());
    assert_eq!(two.height(), one.height());
}

#[test]
fn a_filter_badge_is_a_toggle_that_says_whether_it_is_on() {
    let mut h = harness(Density::Comfortable);
    let on = |h: &Harness<'static, Page>| {
        let chip = h.get_by_role_and_label(Role::Button, "3 Errors");
        chip.accesskit_node().toggled() == Some(Toggled::True)
    };
    assert!(on(&h));
    h.get_by_role_and_label(Role::Button, "3 Errors").click();
    h.run();
    assert!(!h.state().errors_shown);
    assert!(!on(&h));
    // Off, it keeps its size, so the header does not move.
    let off = h.get_by_role_and_label(Role::Button, "3 Errors").rect();
    h.get_by_role_and_label(Role::Button, "3 Errors").click();
    h.run();
    assert_eq!(h.get_by_label("3 Errors").rect(), off);
}

#[test]
fn a_colour_button_is_a_named_swatch() {
    // The picker itself is egui's, and is not opened here: its copy button is a
    // character REDA's fonts do not have, which a test harness refuses to draw.
    let h = harness(Density::Comfortable);
    let button = h.get_by_role_and_label(Role::Button, "Colour of Top Copper");
    assert_eq!(button.rect().size(), egui::Vec2::splat(widgets::SWATCH));
    assert_eq!(h.state().recolours, 0);
}

#[test]
fn the_swatch_of_no_colour_is_a_swatch_sized_button() {
    let h = harness(Density::Comfortable);
    let none = h.get_by_role_and_label(Role::Button, "No colour");
    assert_eq!(none.rect().size(), egui::Vec2::splat(widgets::SWATCH));
}
