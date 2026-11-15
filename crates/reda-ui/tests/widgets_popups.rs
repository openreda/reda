//! The popups and containers, driven through AccessKit: menus, context menus, submenus,
//! tooltips, the dialog frame and its button row. How they look is the gallery's job
//! (`tests/widget_gallery`).

use egui::accesskit::Role;
use egui::{Id, Theme, Ui};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::modal;
use reda_ui::theme::{self, Density, Settings, TokensExt as _};
use reda_ui::widgets::{self, Tooltip as _};

/// What the form below changes.
#[derive(Default)]
struct Form {
    ran: Vec<&'static str>,
    snap: bool,
    layer: u8,
    details_open: bool,
    dialog_closed: bool,
    answer: Option<&'static str>,
    /// Where the dialog's buttons were drawn.
    buttons: Vec<(&'static str, egui::Rect)>,
    dialog: Option<egui::Rect>,
}

fn items(ui: &mut Ui, f: &mut Form) {
    if ui
        .add(widgets::menu_item("Undo").icon("undo").shortcut("Ctrl+Z"))
        .clicked()
    {
        f.ran.push("undo");
    }
    ui.add_enabled(false, widgets::menu_item("Redo").shortcut("Ctrl+Shift+Z"))
        .disabled_tooltip("Nothing to redo");
    widgets::menu_separator(ui);
    if ui
        .add(widgets::menu_item("Snap to Grid").checked(f.snap))
        .clicked()
    {
        f.snap = !f.snap;
    }
    widgets::menu_button("Find Similar").show(ui, |ui| {
        if ui.add(widgets::menu_item("Same Net")).clicked() {
            f.ran.push("same-net");
        }
        if ui.add(widgets::menu_item("Same Layer")).clicked() {
            f.ran.push("same-layer");
        }
    });
    if ui.add(widgets::menu_item("Delete")).clicked() {
        f.ran.push("delete");
    }
}

fn form(ui: &mut Ui, f: &mut Form) {
    widgets::menu_bar(ui, |ui| {
        widgets::menu_button("Edit").show(ui, |ui| items(ui, f));
        widgets::menu_button("View").show(ui, |ui| {
            if ui.add(widgets::menu_item("Zoom to Fit")).clicked() {
                f.ran.push("fit");
            }
        });
    });
    widgets::menu_button("New").show(ui, |ui| {
        if ui.add(widgets::menu_item("Symbol")).clicked() {
            f.ran.push("new-symbol");
        }
    });
    let target = ui.add(widgets::button("Canvas"));
    widgets::context_menu(&target, |ui| {
        if ui.add(widgets::menu_item("Properties")).clicked() {
            f.ran.push("properties");
        }
    });
    ui.add(widgets::button("Route"))
        .tooltip_key("Route Track", "X");
    let layer_name = ["Top", "Inner", "Bottom"][usize::from(f.layer)];
    let layer = widgets::dropdown("layer", layer_name).show(ui, |ui| {
        widgets::choice_value(ui, &mut f.layer, 0, "Top");
        widgets::choice_value(ui, &mut f.layer, 1, "Inner");
        widgets::choice_value(ui, &mut f.layer, 2, "Bottom");
    });
    layer
        .response
        .widget_info(|| egui::WidgetInfo::labeled(Role::ComboBox, true, "Layer"));
}

fn windows(ctx: &egui::Context, f: &mut Form) {
    widgets::window(Id::NULL.with("details"), "Details")
        .open(&mut f.details_open)
        .default_pos(egui::pos2(500.0, 200.0))
        .show(ctx, |ui| {
            ui.label("What went wrong.");
        });
    if f.answer.is_none() && !f.dialog_closed && f.ran.contains(&"delete") {
        let shown =
            modal::Dialog::new(Id::NULL.with("confirm"), "Delete 14 Tracks?").show(ctx, |ui| {
                ui.label("This can be undone.");
                let mut drawn = Vec::new();
                modal::buttons(ui, |ui| {
                    let delete = ui.add(widgets::button("Delete").danger());
                    let cancel = ui.add(widgets::button("Cancel"));
                    let keep = ui.add(widgets::button("Keep Locked"));
                    drawn = vec![
                        ("Delete", delete.rect),
                        ("Cancel", cancel.rect),
                        ("Keep Locked", keep.rect),
                    ];
                    if delete.clicked() {
                        f.answer = Some("delete");
                    }
                    if cancel.clicked() {
                        f.answer = Some("cancel");
                    }
                });
                f.buttons = drawn;
                ui.min_rect()
            });
        f.dialog = Some(shown.inner);
        f.dialog_closed = shown.should_close();
    }
}

fn harness() -> Harness<'static, Form> {
    let mut installed = false;
    let state = Form {
        details_open: true,
        ..Form::default()
    };
    let mut h = Harness::builder()
        .with_size([1000.0, 700.0])
        .build_ui_state(
            move |ui, f: &mut Form| {
                if !installed {
                    theme::install(ui.ctx(), Theme::Light.into());
                    theme::set(
                        ui.ctx(),
                        &Settings {
                            density: Density::Comfortable,
                            ..Settings::default()
                        },
                    );
                    installed = true;
                }
                form(ui, f);
                let ctx = ui.ctx().clone();
                windows(&ctx, f);
            },
            state,
        );
    h.run();
    h
}

fn button<'h>(h: &'h Harness<'_, Form>, label: &'h str) -> egui_kittest::Node<'h> {
    h.get_by_role_and_label(Role::Button, label)
}

#[test]
fn a_menu_button_opens_its_menu_and_an_item_runs_and_closes_it() {
    let mut h = harness();
    assert!(h.query_by_label("Undo Ctrl+Z").is_none());
    button(&h, "Edit").click();
    h.run();
    // An item with a shortcut is read out with it.
    button(&h, "Undo Ctrl+Z").click();
    h.run();
    assert_eq!(h.state().ran, ["undo"]);
    assert!(h.query_by_label("Undo Ctrl+Z").is_none(), "the menu closed");
}

#[test]
fn a_disabled_item_does_nothing_and_a_checked_one_says_so() {
    let mut h = harness();
    button(&h, "Edit").click();
    h.run();
    assert!(
        button(&h, "Redo Ctrl+Shift+Z")
            .accesskit_node()
            .is_disabled()
    );
    let snap = button(&h, "Snap to Grid");
    assert_eq!(
        snap.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::False)
    );
    snap.click();
    h.run();
    assert!(h.state().snap);
    button(&h, "Edit").click();
    h.run();
    assert_eq!(
        button(&h, "Snap to Grid").accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
}

#[test]
fn a_submenu_opens_beside_its_item_and_its_items_run() {
    let mut h = harness();
    button(&h, "Edit").click();
    h.run();
    assert!(h.query_by_label("Same Net").is_none());
    button(&h, "Find Similar").click();
    h.run();
    let parent = button(&h, "Find Similar").rect();
    let child = button(&h, "Same Net").rect();
    assert!(child.left() >= parent.right(), "{parent:?} then {child:?}");
    button(&h, "Same Layer").click();
    h.run();
    assert_eq!(h.state().ran, ["same-layer"]);
    assert!(
        h.query_by_label("Undo Ctrl+Z").is_none(),
        "the whole menu closed"
    );
}

#[test]
fn the_arrow_keys_move_through_a_menu_and_into_a_submenu() {
    let mut h = harness();
    button(&h, "Edit").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert!(button(&h, "Undo Ctrl+Z").is_focused());
    // Down skips the disabled Redo.
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert!(button(&h, "Snap to Grid").is_focused());
    h.key_press(egui::Key::ArrowDown);
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert!(button(&h, "Same Net").is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().ran, ["same-net"]);
}

#[test]
fn hovering_another_title_of_a_menu_bar_switches_menus() {
    let mut h = harness();
    button(&h, "Edit").click();
    h.run();
    button(&h, "View").hover();
    h.run();
    assert!(h.query_by_label("Undo Ctrl+Z").is_none());
    button(&h, "Zoom to Fit").click();
    h.run();
    assert_eq!(h.state().ran, ["fit"]);
}

#[test]
fn a_menu_button_on_its_own_opens_a_menu_too() {
    let mut h = harness();
    button(&h, "New").click();
    h.run();
    button(&h, "Symbol").click();
    h.run();
    assert_eq!(h.state().ran, ["new-symbol"]);
}

#[test]
fn a_secondary_click_opens_a_context_menu() {
    let mut h = harness();
    assert!(h.query_by_label("Properties").is_none());
    button(&h, "Canvas").click_secondary();
    h.run();
    button(&h, "Properties").click();
    h.run();
    assert_eq!(h.state().ran, ["properties"]);
}

#[test]
fn escape_closes_a_menu() {
    let mut h = harness();
    button(&h, "Edit").click();
    h.run();
    h.key_press(egui::Key::Escape);
    h.run();
    assert!(h.query_by_label("Undo Ctrl+Z").is_none());
}

#[test]
fn the_arrow_keys_move_through_a_dropdowns_list_and_enter_takes_the_row() {
    let mut h = harness();
    h.get_by_role_and_label(Role::ComboBox, "Layer").focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert!(button(&h, "Top").is_focused());
    h.key_press(egui::Key::ArrowDown);
    h.run();
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert!(button(&h, "Bottom").is_focused());
    h.key_press(egui::Key::Enter);
    h.run();
    assert_eq!(h.state().layer, 2);
    assert!(h.query_by_label("Inner").is_none(), "the list closed");
}

#[test]
fn a_tooltip_shows_a_name_and_its_key_when_the_widget_is_hovered() {
    let mut h = harness();
    assert!(h.query_by_label("Route Track").is_none());
    button(&h, "Route").hover();
    h.run_steps(90);
    h.get_by_label("Route Track");
    h.get_by_label("X");
}

#[test]
fn a_window_shows_its_title_and_its_close_button_closes_it() {
    let mut h = harness();
    h.get_by_role_and_label(Role::Window, "Details");
    h.get_by_label("What went wrong.");
    button(&h, "Close window").click();
    h.run();
    assert!(!h.state().details_open);
    assert!(h.query_by_label("What went wrong.").is_none());
}

/// Opens the modal dialog of the form.
fn confirming() -> Harness<'static, Form> {
    let mut h = harness();
    h.state_mut().details_open = false;
    button(&h, "Edit").click();
    h.run();
    button(&h, "Delete").click();
    h.run();
    h
}

#[test]
fn a_dialogs_buttons_are_right_aligned_with_the_primary_one_last() {
    let h = confirming();
    let f = h.state();
    let t = h.ctx.tokens();
    let [(_, delete), (_, cancel), (_, keep)] = f.buttons[..] else {
        panic!("three buttons: {:?}", f.buttons);
    };
    assert!(
        (delete.right() - f.dialog.unwrap().right()).abs() < 0.5,
        "{delete:?}"
    );
    assert!((delete.left() - cancel.right() - t.space.gap).abs() < 0.5);
    assert!((cancel.left() - keep.right() - t.space.gap).abs() < 0.5);
    assert_eq!(delete.height(), t.control_height());
}

#[test]
fn a_dialogs_close_button_closes_it_and_what_is_under_it_takes_no_input() {
    let mut h = confirming();
    // The scrim is over the form: the menu button under it cannot be clicked.
    h.get_by_role_and_label(Role::Window, "Delete 14 Tracks?");
    button(&h, "Close window").click();
    h.run();
    assert!(h.state().dialog_closed);
    assert_eq!(h.state().answer, None);
}

#[test]
fn a_dialogs_button_answers_it() {
    let mut h = confirming();
    button(&h, "Cancel").click();
    h.run();
    assert_eq!(h.state().answer, Some("cancel"));
}

#[test]
fn shadows_are_the_elevation_tokens_and_only_what_floats_has_one() {
    let h = harness();
    let t = h.ctx.tokens();
    assert_eq!(widgets::card_frame(&t).shadow, t.elevation.card);
    assert_eq!(widgets::floating_frame(&t).shadow, t.elevation.popup);
    assert_eq!(widgets::flyout_frame(&t).shadow, t.elevation.popup);
    assert_eq!(widgets::menu_frame(&t).shadow, t.elevation.popup);
    assert_eq!(widgets::tooltip_frame(&t).shadow, t.elevation.popup);
    assert_eq!(widgets::dialog_frame(&t).shadow, t.elevation.window);
    // The spec's values (docs/spec/v0.1/design-language.md#elevation-values), in light.
    let shadow = |y: i8, blur: u8| (y, blur);
    let of = |s: egui::Shadow| (s.offset[1], s.blur);
    assert_eq!(of(t.elevation.card), shadow(1, 4));
    assert_eq!(of(t.elevation.popup), shadow(6, 24));
    assert_eq!(of(t.elevation.window), shadow(12, 40));
    // What stock egui still draws takes the same shadows.
    let style = h.ctx.global_style();
    assert_eq!(style.visuals.popup_shadow, t.elevation.popup);
    assert_eq!(style.visuals.window_shadow, t.elevation.window);
}
