//! The Layer chip and the strip that holds them, driven through AccessKit. How they look
//! is the gallery's job (`tests/widget_gallery/canvas_chrome.rs`).

use egui::accesskit::{Role, Toggled};
use egui::{Rect, Ui, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::widgets;

const LAYERS: [&str; 3] = ["Top Copper", "Bottom Copper", "Top Silkscreen"];

/// A strip of chips along the bottom of a 600 × 200 area: the active Layer is the one
/// last clicked. The second chip shows only its swatch and is named for assistive tech.
fn strip(ui: &mut Ui, active: &mut usize) {
    let rect = Rect::from_min_size(pos2(0.0, 176.0), vec2(600.0, 24.0));
    widgets::chip_strip(ui, rect, |ui| {
        ui.add(widgets::chip("Layer sets").chevron());
        widgets::chip_rule(ui);
        for (i, name) in LAYERS.into_iter().enumerate() {
            let text = if i == 1 { "" } else { name };
            let chip = widgets::chip(text)
                .swatch_rgb(0xc8, 0x34, 0x34)
                .selected(*active == i)
                .name(format!("Layer: {name}"));
            if ui.add(chip).clicked() {
                *active = i;
            }
        }
    });
}

fn harness() -> Harness<'static, usize> {
    Harness::builder()
        .with_size([600.0, 200.0])
        .build_ui_state(strip, 0)
}

#[test]
fn a_chip_is_a_button_named_for_its_layer_that_reports_the_active_one() {
    let mut h = harness();
    h.run();
    let chip = h.get_by_role_and_label(Role::Button, "Layer: Top Copper");
    assert_eq!(chip.accesskit_node().toggled(), Some(Toggled::True));
    let swatch_only = h.get_by_role_and_label(Role::Button, "Layer: Bottom Copper");
    assert_ne!(swatch_only.accesskit_node().toggled(), Some(Toggled::True));
    swatch_only.click();
    h.run();
    assert_eq!(*h.state(), 1);
    let chip = h.get_by_role_and_label(Role::Button, "Layer: Bottom Copper");
    assert_eq!(chip.accesskit_node().toggled(), Some(Toggled::True));
}

#[test]
fn the_chips_sit_in_one_row_inside_the_strip() {
    let mut h = harness();
    h.run();
    let strip = Rect::from_min_size(pos2(0.0, 176.0), vec2(600.0, 24.0));
    let mut rects = vec![h.get_by_role_and_label(Role::Button, "Layer sets").rect()];
    rects.extend(LAYERS.map(|name| {
        h.get_by_role_and_label(Role::Button, &format!("Layer: {name}"))
            .rect()
    }));
    for pair in rects.windows(2) {
        assert!(
            pair[0].right() <= pair[1].left(),
            "left to right: {rects:?}"
        );
    }
    for rect in &rects {
        assert!(strip.contains_rect(*rect), "{rect:?} inside {strip:?}");
    }
    // The swatch-only chip is narrower than a named one.
    assert!(rects[2].width() < rects[1].width());
}
