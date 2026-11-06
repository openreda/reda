//! Modal dialogs open centred at their default size, and can be moved by their title and
//! resized by their edges, down to a minimum their content still fits.

use egui::{Id, Pos2, Rect, Vec2, pos2, vec2};
use egui_kittest::Harness;
use reda_ui::modal::{Dialog, MIN_SIZE};

fn id() -> Id {
    Id::NULL.with("test-dialog")
}

fn harness() -> Harness<'static> {
    let mut h = Harness::builder().with_size([800.0, 600.0]).build_ui(|ui| {
        let ctx = ui.ctx().clone();
        Dialog::new(id(), "A Dialog").width(300.0).show(&ctx, |ui| {
            ui.label("What it asks.");
        });
    });
    h.run();
    h
}

fn rect(h: &Harness<'static>) -> Rect {
    h.ctx
        .memory(|m| m.area_rect(id()))
        .expect("the dialog was drawn")
}

fn drag(h: &mut Harness<'static>, from: Pos2, by: Vec2) {
    h.hover_at(from);
    h.run();
    h.drag_at(from);
    h.run();
    for i in 1..=4 {
        h.hover_at(from + by * (i as f32 / 4.0));
        h.run();
    }
    h.drop_at(from + by);
    h.run();
}

#[test]
fn a_dialog_opens_centred_at_its_default_width() {
    let h = harness();
    let r = rect(&h);
    assert!(
        (r.center() - pos2(400.0, 300.0)).length() < 2.0,
        "centred: {r:?}"
    );
    assert!(
        (r.width() - 300.0).abs() < 30.0,
        "about its default width: {r:?}"
    );
}

#[test]
fn a_dialog_moves_by_its_title() {
    let mut h = harness();
    let before = rect(&h);
    let title = pos2(before.center().x, before.top() + 10.0);
    drag(&mut h, title, vec2(-120.0, -80.0));
    let after = rect(&h);
    assert!(
        (after.min - before.min - vec2(-120.0, -80.0)).length() < 2.0,
        "moved with the pointer: {before:?} to {after:?}"
    );
}

#[test]
fn a_dialog_resizes_by_its_edges_down_to_a_minimum_its_content_fits() {
    let mut h = harness();
    let before = rect(&h);
    let edge = pos2(before.right() - 1.0, before.center().y);
    drag(&mut h, edge, vec2(100.0, 0.0));
    let wider = rect(&h);
    assert!(
        wider.width() > before.width() + 80.0,
        "{before:?} to {wider:?}"
    );

    let edge = pos2(wider.right() - 1.0, wider.center().y);
    drag(&mut h, edge, vec2(-600.0, 0.0));
    let smallest = rect(&h);
    assert!(smallest.width() >= MIN_SIZE.x - 1.0, "{smallest:?}");
    // Its height follows the content: never shorter than it was.
    assert!(smallest.height() >= before.height() - 1.0, "{smallest:?}");
}
