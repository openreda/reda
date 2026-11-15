//! Close-ups for a reviewer, at twice the size: the three controls that the component
//! gallery never drew, in every state and live with the list open. Written to
//! `REDA_GALLERY_OUT`; nothing compares them.

use egui::{Role, Ui, vec2};
use egui_kittest::kittest::Queryable as _;
use reda_ui::theme::TokensExt as _;
use reda_ui::widgets;

use crate::controls::{self, DENSITIES, THEMES};
use crate::{Variant, harness, keep, kit};

const SCALE: f32 = 2.0;

/// What the live sheet edits.
struct Form {
    theme: u8,
    density: u8,
    text_size: i64,
    font: usize,
}

fn form(ui: &mut Ui, form: &mut Form) {
    let t = ui.tokens();
    kit::section(ui, "Live: as on the Appearance page", |ui| {
        ui.spacing_mut().item_spacing.y = t.space.stack;
        let name = |ui: &mut Ui, text: &str| {
            ui.allocate_ui_with_layout(
                vec2(110.0, t.control_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(110.0);
                    ui.label(t.ty.body.rich(text).color(t.color.text.primary));
                },
            );
        };
        ui.horizontal(|ui| {
            name(ui, "Theme");
            ui.add(widgets::segmented(&mut form.theme, &THEMES));
        });
        ui.horizontal(|ui| {
            name(ui, "Density");
            ui.add(widgets::segmented(&mut form.density, &DENSITIES));
        });
        ui.horizontal(|ui| {
            name(ui, "Text size");
            let stepper = widgets::stepper_field(&mut form.text_size, 11..=16).suffix("px");
            let stepper = ui.add(stepper);
            stepper.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Text size")
            });
        });
        ui.horizontal(|ui| {
            name(ui, "UI font");
            let fonts = controls::fonts();
            let picked = widgets::filter_dropdown("sheet-font", fonts[form.font].text)
                .width(220.0)
                .hint("Filter")
                .none_text("No fonts match")
                .show(ui, &fonts, Some(form.font));
            picked
                .response
                .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, "UI font"));
            if let Some(index) = picked.index {
                form.font = index;
            }
        });
    });
}

pub fn write_all() {
    for variant in Variant::ALL {
        let name = variant.name();

        // Every state, forced side by side.
        let mut h = harness(variant, vec2(1580.0, 700.0), SCALE, |ui| {
            kit::section(
                ui,
                "Segmented control, stepper field and filterable dropdown",
                controls::new_controls,
            );
        });
        h.run();
        keep(&mut h, &format!("states_{name}"));

        // Live, driven as a user would.
        let mut state = Form {
            theme: 1,
            density: 0,
            text_size: 13,
            font: 0,
        };
        let mut h = harness(variant, vec2(480.0, 470.0), SCALE, move |ui| {
            form(ui, &mut state);
        });
        h.run();
        keep(&mut h, &format!("live_1_at_rest_{name}"));

        h.get_by_label("Compact").hover();
        h.run();
        keep(&mut h, &format!("live_2_segment_hovered_{name}"));

        h.get_by_label("Compact").click();
        h.run();
        h.get_by_role_and_label(Role::TextInput, "Text size")
            .focus();
        h.run();
        h.key_press(egui::Key::ArrowUp);
        h.run();
        keep(
            &mut h,
            &format!("live_3_segment_picked_stepper_focused_{name}"),
        );

        h.get_by_role_and_label(Role::ComboBox, "UI font").click();
        h.run_steps(4);
        keep(&mut h, &format!("live_4_list_open_{name}"));

        h.get_by_role_and_label(Role::SearchInput, "Filter")
            .type_text("sans");
        h.run_steps(2);
        h.key_press(egui::Key::ArrowDown);
        h.run_steps(2);
        keep(&mut h, &format!("live_5_list_filtered_{name}"));

        h.get_by_role_and_label(Role::SearchInput, "Filter")
            .type_text("zz");
        h.run_steps(2);
        keep(&mut h, &format!("live_6_list_no_match_{name}"));
    }
}
