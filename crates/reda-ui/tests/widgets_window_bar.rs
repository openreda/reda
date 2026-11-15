//! The window controls, the slim bar of a floating window and the command field, driven
//! through AccessKit. How they look is the gallery's job (`tests/widget_gallery`).

use egui::accesskit::Role;
use egui::{Theme, Ui, ViewportCommand};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::theme::{self, TokensExt as _};
use reda_ui::widgets::{self, WindowAction};

#[derive(Default)]
struct Page {
    maximised: bool,
    controls: Vec<WindowAction>,
    floating: Vec<WindowAction>,
    field_clicks: u32,
}

fn page(ui: &mut Ui, p: &mut Page) {
    if let Some(action) = widgets::window_controls(p.maximised).show(ui) {
        p.controls.push(action);
    }
    let bar = widgets::floating_bar("Layers — Board")
        .maximised(p.maximised)
        .show(ui);
    if let Some(action) = bar.action {
        p.floating.push(action);
    }
    let field = widgets::command_field("Board — Sheet 1")
        .name("Search Documents, Components, Nets")
        .width(300.0);
    if ui.add(field).clicked() {
        p.field_clicks += 1;
    }
}

fn harness() -> Harness<'static, Page> {
    let mut installed = false;
    let mut h = Harness::builder().with_size([600.0, 300.0]).build_ui_state(
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
fn the_window_controls_are_46_px_wide_and_say_what_was_clicked() {
    let mut h = harness();
    let height = h.ctx.tokens().title_bar_height();
    let names = ["Minimise", "Maximise", "Close"];
    let rects: Vec<_> = names
        .iter()
        .map(|n| {
            h.get_all_by_role_and_label(Role::Button, n)
                .next()
                .unwrap()
                .rect()
        })
        .collect();
    for (rect, name) in rects.iter().zip(names) {
        assert_eq!((rect.width(), rect.height()), (46.0, height), "{name}");
    }
    // Side by side, in that order, with nothing between them.
    assert_eq!(rects[0].right(), rects[1].left());
    assert_eq!(rects[1].right(), rects[2].left());
    for name in names {
        h.get_all_by_role_and_label(Role::Button, name)
            .next()
            .unwrap()
            .click();
        h.run();
    }
    assert_eq!(
        h.state().controls,
        [
            WindowAction::Minimise,
            WindowAction::Maximise,
            WindowAction::Close
        ]
    );
}

#[test]
fn a_maximised_window_shows_restore_in_place_of_maximise() {
    let mut h = harness();
    h.state_mut().maximised = true;
    h.run();
    assert!(h.query_by_label("Maximise").is_none());
    h.get_all_by_role_and_label(Role::Button, "Restore")
        .next()
        .unwrap()
        .click();
    h.run();
    assert_eq!(h.state().controls, [WindowAction::Restore]);
}

#[test]
fn each_action_is_the_window_command_it_names() {
    assert_eq!(
        WindowAction::Minimise.command(),
        ViewportCommand::Minimized(true)
    );
    assert_eq!(
        WindowAction::Maximise.command(),
        ViewportCommand::Maximized(true)
    );
    assert_eq!(
        WindowAction::Restore.command(),
        ViewportCommand::Maximized(false)
    );
    assert_eq!(WindowAction::Close.command(), ViewportCommand::Close);
}

#[test]
fn the_floating_bar_is_32_px_with_its_title_and_the_same_controls() {
    let mut h = harness();
    let bar = h
        .get_by_role_and_label(Role::TitleBar, "Layers — Board")
        .rect();
    assert_eq!(bar.height(), 32.0);
    let close = h
        .get_all_by_role_and_label(Role::Button, "Close")
        .nth(1)
        .unwrap();
    let rect = close.rect();
    assert_eq!((rect.width(), rect.height()), (46.0, 32.0));
    assert_eq!(rect.right(), bar.right(), "at the bar's trailing end");
    close.click();
    h.run();
    assert_eq!(h.state().floating, [WindowAction::Close]);
    assert!(h.state().controls.is_empty());
}

#[test]
fn the_command_field_is_named_by_its_prompt_and_shows_the_title() {
    let mut h = harness();
    let node = h.get_by_role_and_label(Role::Button, "Search Documents, Components, Nets");
    assert_eq!(
        node.accesskit_node().value().as_deref(),
        Some("Board — Sheet 1")
    );
    assert_eq!(node.rect().width(), 300.0);
    node.click();
    h.run();
    assert_eq!(h.state().field_clicks, 1);
}
