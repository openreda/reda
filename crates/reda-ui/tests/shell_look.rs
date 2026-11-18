//! The look of the window shell below the Title bar, checked on what a frame paints
//! (docs/spec/v0.1/design-language.md#tabs, #docks, #status-bar): tab groups as cards a
//! card gap apart, the tab strip's height in both Densities, a collapsed Dock's strip, the
//! status bar, the corner masks over a card that holds a canvas, the drop-target recipe
//! while a tab is dragged, the tab switcher and the focus ring.

use egui::accesskit::Role;
use egui::epaint::{ClippedShape, RectShape};
use egui::{Color32, CornerRadius, Event, Key, Modifiers, Rect, Shape, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings, TokensExt as _};
use serde_json::json;

const SIZE: [f32; 2] = [1000.0, 640.0];
/// The test harness keeps its own 8 px margin around the window it draws.
const WINDOW: Rect = Rect::from_min_max(pos2(8.0, 8.0), pos2(SIZE[0] - 8.0, SIZE[1] - 8.0));

fn engine() -> Engine {
    Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .headless(false)
        .build()
}

fn harness_of(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size(SIZE)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn harness() -> Harness<'static, App> {
    harness_of(Shell::new(engine()))
}

/// A window with a Project open on its first sheet.
fn harness_with_sheet(work: &TempDir) -> Harness<'static, App> {
    let mut shell = Shell::new(engine());
    shell.new_project(work.path(), "Look").unwrap();
    harness_of(shell)
}

fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
        other => out.push(other.clone()),
    }
}

/// Every shape painted this frame, in paint order.
fn shapes(h: &Harness<'_, App>) -> Vec<Shape> {
    let mut out = Vec::new();
    for ClippedShape { shape, .. } in &h.output().shapes {
        flatten(shape, &mut out);
    }
    out
}

fn rects(h: &Harness<'_, App>) -> Vec<RectShape> {
    shapes(h)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Rect(r) => Some(r),
            _ => None,
        })
        .collect()
}

/// The cards painted this frame: rects filled with the card colour and rounded by the
/// card radius on every corner. Sorted from the left, then from the top.
fn cards(h: &Harness<'_, App>) -> Vec<Rect> {
    let card = h.ctx.tokens().color.surface.card;
    let mut cards: Vec<Rect> = rects(h)
        .into_iter()
        .filter(|r| r.fill == card && r.corner_radius == CornerRadius::same(12))
        .map(|r| r.rect)
        .collect();
    cards.sort_by(|a, b| {
        (a.left(), a.top())
            .partial_cmp(&(b.left(), b.top()))
            .unwrap()
    });
    cards.dedup();
    cards
}

fn set_density(h: &mut Harness<'_, App>, density: Density) {
    let settings = Settings {
        density,
        ..Settings::default()
    };
    theme::set(&h.ctx, &settings);
    h.run();
}

#[test]
fn tab_groups_are_cards_six_apart_and_six_from_the_window_edges() {
    let h = harness();
    // The Default Layout: Projects | Start | Properties over Messages.
    let cards = cards(&h);
    assert_eq!(cards.len(), 4, "one card per tab group: {cards:?}");
    let bottom = *cards
        .iter()
        .max_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    let mut top: Vec<Rect> = cards.iter().copied().filter(|c| *c != bottom).collect();
    top.sort_by(|a, b| a.left().total_cmp(&b.left()));
    assert_eq!(top[0].left() - WINDOW.left(), 6.0, "from the window's edge");
    assert_eq!(top[1].left() - top[0].right(), 6.0);
    assert_eq!(top[2].left() - top[1].right(), 6.0);
    assert_eq!(WINDOW.right() - top[2].right(), 6.0);
    assert_eq!(bottom.left() - WINDOW.left(), 6.0);
    assert_eq!(WINDOW.right() - bottom.right(), 6.0);
    // Below the upper cards: the gap, then the bottom group's tab strip, then its card.
    let strip = h.get_by_role_and_label(Role::Tab, "Messages").rect();
    assert_eq!(strip.top() - top[1].bottom(), 6.0);
    assert_eq!(strip.bottom(), bottom.top());
}

#[test]
fn tabs_are_34_px_in_comfortable_and_30_px_in_compact_and_sit_on_their_card() {
    let mut h = harness();
    for (density, height) in [(Density::Comfortable, 34.0), (Density::Compact, 30.0)] {
        set_density(&mut h, density);
        let cards = cards(&h);
        for tab in ["Projects", "Start", "Properties", "Messages"] {
            let rect = h.get_by_role_and_label(Role::Tab, tab).rect();
            assert_eq!(rect.height(), height, "{tab} in {density:?}");
            assert!(
                cards
                    .iter()
                    .any(|c| c.top() == rect.bottom() && c.left() + 6.0 == rect.left()),
                "{tab} is joined to the card below it, 6 px in from its edge"
            );
        }
    }
}

#[test]
fn no_tab_strip_is_filled_with_a_colour_of_its_own() {
    let h = harness();
    let t = h.ctx.tokens();
    let tab = h.get_by_role_and_label(Role::Tab, "Start").rect();
    // Whatever is painted across the strip beside the tabs is the window's base.
    let neutral = [
        t.color.surface.base,
        t.color.surface.card,
        Color32::TRANSPARENT,
    ];
    for r in rects(&h) {
        let across =
            r.rect.width() > 400.0 && r.rect.top() <= tab.top() && r.rect.bottom() >= tab.bottom();
        assert!(
            !across || neutral.contains(&r.fill),
            "a bar of {:?} behind the tabs",
            r.fill
        );
    }
}

#[test]
fn a_collapsed_dock_is_a_strip_of_20_px_icon_buttons_with_the_open_flyouts_on() {
    let mut h = harness();
    let shell = h.state_mut().shell_mut();
    shell
        .run("layout.dock.collapse", json!({"dock": "right"}))
        .unwrap();
    h.run();
    let t = h.ctx.tokens();
    let button = h.get_by_role_and_label(Role::Button, "Properties").rect();
    assert_eq!(
        button.size(),
        egui::vec2(30.0, 30.0),
        "a control-height square"
    );
    assert_eq!(WINDOW.right() - button.right(), 6.0, "a card gap in");
    let icon = Rect::from_center_size(button.center(), egui::Vec2::splat(20.0));
    assert!(
        shapes(&h)
            .iter()
            .any(|s| matches!(s, Shape::Rect(r) if r.rect == icon && r.fill_texture_id() != Default::default())),
        "a 20 px Icon in the button"
    );
    // No frame and no ground of the strip's own: nothing is painted on it but buttons.
    let on = |h: &Harness<'_, App>| {
        let button = h.get_by_role_and_label(Role::Button, "Properties").rect();
        rects(h)
            .iter()
            .any(|r| r.rect == button && r.fill == t.color.accent.subtle)
    };
    assert!(!on(&h), "off while its flyout is closed");
    h.get_by_role_and_label(Role::Button, "Properties").click();
    h.run();
    // Off the button, so that its hover fill is not over the on fill.
    h.event(Event::PointerGone);
    h.run();
    assert!(on(&h), "the Icon of the open flyout is on");
}

#[test]
fn the_status_bar_is_24_px_of_flat_items_on_the_base_with_no_rule() {
    let work = TempDir::new("look-status");
    let h = harness_with_sheet(&work);
    let t = h.ctx.tokens();
    let panels = h.get_by_role_and_label(Role::Button, "Panels").rect();
    assert_eq!(panels.height(), 24.0);
    assert_eq!(panels.bottom(), WINDOW.bottom());
    let kind = h
        .get_by_role_and_label(Role::Button, "Active: Schematic Sheet")
        .rect();
    assert_eq!((kind.top(), kind.bottom()), (panels.top(), panels.bottom()));
    // Nothing is filled or ruled across the bar: only the base is under its items.
    let bar = WINDOW.with_min_y(panels.top() - 1.0);
    for shape in shapes(&h) {
        match shape {
            Shape::Rect(r) if bar.contains_rect(r.rect) && r.rect.width() > 400.0 => {
                assert_eq!(r.fill, t.color.surface.base, "the bar's ground");
                assert!(r.stroke.is_empty(), "no rim");
            }
            Shape::LineSegment { points, .. } => {
                let ruled = points[0].y == points[1].y
                    && (points[0].y - panels.top()).abs() <= 1.0
                    && (points[0].x - points[1].x).abs() > 400.0;
                assert!(!ruled, "a rule above the status bar");
            }
            _ => {}
        }
    }
}

/// The corner masks painted this frame, as the boxes their wedges span.
fn masks(h: &Harness<'_, App>) -> Vec<Rect> {
    let base = h.ctx.tokens().color.surface.base;
    shapes(h)
        .into_iter()
        .filter_map(|s| match s {
            Shape::Mesh(mesh) if mesh.vertices.iter().all(|v| v.color == base) => {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_card_that_holds_a_canvas_is_masked_to_its_corners_in_every_window() {
    let work = TempDir::new("look-mask");
    let mut h = harness_with_sheet(&work);
    let canvas = h.get_by_role(Role::Canvas).rect();
    let card = *cards(&h)
        .iter()
        .find(|c| c.contains_rect(canvas))
        .expect("the canvas is in a card");
    assert_eq!(masks(&h), [card], "the sheet's card, and no Panel's");

    // The same in a floating window.
    let shell = h.state_mut().shell_mut();
    let sheet = shell.layout().active_document().unwrap();
    shell
        .run(
            "layout.tab.move_to_new_window",
            json!({"tab": sheet.to_string()}),
        )
        .unwrap();
    h.run_steps(4);
    let canvas = h.get_by_role(Role::Canvas).rect();
    let card = *cards(&h)
        .iter()
        .find(|c| c.contains_rect(canvas))
        .expect("the floating canvas is in a card");
    assert!(masks(&h).contains(&card), "{:?} has {card:?}", masks(&h));
}

#[test]
fn the_tab_switcher_is_a_popup_of_rows_with_the_pick_selected() {
    let mut h = harness();
    h.event(Event::ModifiersChanged(Modifiers::CTRL));
    h.event(Event::Key {
        key: Key::Tab,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    });
    h.run();
    h.run();
    let t = h.ctx.tokens();
    let rows: Vec<Rect> = h
        .get_by_role_and_label(Role::List, "Switch Tab")
        .query_all_by_role(Role::ListItem)
        .map(|n| n.rect())
        .collect();
    assert!(rows.len() >= 2);
    let list = rows[0].union(rows[rows.len() - 1]);
    let popup = rects(&h)
        .into_iter()
        .find(|r| r.fill == t.color.surface.floating && r.rect.contains_rect(list))
        .expect("the switcher is on the floating surface");
    assert_eq!(
        popup.corner_radius,
        CornerRadius::same(12),
        "the popup radius"
    );
    assert!(
        rows.iter().all(|r| r.height() == 28.0),
        "list rows: {rows:?}"
    );
    let picked: Vec<RectShape> = rects(&h)
        .into_iter()
        .filter(|r| rows.contains(&r.rect) && r.fill == t.color.accent.subtle)
        .collect();
    assert_eq!(picked.len(), 1, "the pick is the live selection");
    assert_eq!(
        picked[0].corner_radius,
        CornerRadius::same(6),
        "the row radius"
    );
}

/// The focus rings painted this frame: rects stroked with the focus ring colour.
fn rings(h: &Harness<'_, App>) -> Vec<RectShape> {
    let ring = h.ctx.tokens().color.stroke.focus_ring;
    rects(h)
        .into_iter()
        .filter(|r| r.stroke.color == ring && r.stroke.width > 0.0)
        .collect()
}

#[test]
fn a_focused_tab_has_one_2_px_ring_that_follows_its_radius() {
    let mut h = harness();
    h.get_by_role_and_label(Role::Tab, "Projects").focus();
    h.key_press(Key::Tab);
    h.run();
    let rings = rings(&h);
    assert_eq!(
        rings.len(),
        1,
        "one ring, not the widget's and the window's"
    );
    assert_eq!(rings[0].stroke.width, 2.0);
    // The control radius, grown by the 1 px the ring stands outside.
    assert_eq!(rings[0].corner_radius, CornerRadius::same(9));
}

#[test]
fn a_dragged_tab_shows_the_accents_drop_target_recipe() {
    let mut h = harness();
    let t = h.ctx.tokens();
    let from = h
        .get_by_role_and_label(Role::Tab, "Messages")
        .rect()
        .center();
    let start = h.get_by_role_and_label(Role::Tab, "Start").rect();
    let near = start.center() + egui::vec2(0.0, 60.0);
    h.event(Event::PointerMoved(from));
    h.event(Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    });
    h.run();
    h.event(Event::PointerMoved(from + egui::vec2(20.0, 0.0)));
    h.run();
    h.event(Event::PointerMoved(near));
    h.run();
    h.run();
    // With the pointer on "into this group", the centre's group is the preview.
    let target = h
        .get_by_role_and_label(Role::Button, "Drop into Group")
        .rect();
    h.event(Event::PointerMoved(target.center()));
    h.run();
    h.run();
    let previews: Vec<RectShape> = rects(&h)
        .into_iter()
        .filter(|r| r.fill == t.color.accent.subtle && r.rect.width() > 100.0)
        .collect();
    assert_eq!(previews.len(), 1, "the accent subtle tint over the group");
    assert_eq!(previews[0].stroke.width, 2.0, "inside a 2 px line");
    assert_eq!(
        previews[0].stroke.color, t.color.accent.text,
        "in the accent"
    );
    // The button under the pointer is a drop target too; the others are not.
    let framed = |rect: Rect| {
        rects(&h).iter().any(|r| {
            r.rect == rect && r.stroke.width == 2.0 && r.stroke.color == t.color.accent.text
        })
    };
    assert!(framed(target));
    let left = h
        .get_by_role_and_label(Role::Button, "Drop to the Left")
        .rect();
    assert!(!framed(left));
}
