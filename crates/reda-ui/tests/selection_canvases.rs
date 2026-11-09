//! Real pointer/key gestures through both Library canvas types, using the same rules
//! as the Schematic and PCB canvas tests.

use egui::accesskit::Role;
use egui::{Key, Modifiers, Pos2};
use egui_kittest::{Harness, kittest::Queryable};
use reda_testkit::TempDir;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{LibraryInfo, Uuid};
use reda_ui::{App, engine::Engine, shell::Shell};
use serde_json::{Value, json};

struct Canvas {
    h: Harness<'static, App>,
    document: Uuid,
    small: Uuid,
    large: Uuid,
    _dir: TempDir,
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn canvas(kind: &str) -> Canvas {
    canvas_with(kind, Harness::builder())
}

/// A canvas whose harness takes small steps, so two clicks make a double-click.
fn quick_canvas(kind: &str) -> Canvas {
    canvas_with(
        kind,
        Harness::builder().with_step_dt(0.01).with_max_steps(100),
    )
}

fn canvas_with(kind: &str, builder: egui_kittest::HarnessBuilder<App>) -> Canvas {
    let dir = TempDir::new(&format!("selection-{kind}-canvas"));
    let lib = dir.path().join("library");
    std::fs::create_dir_all(&lib).unwrap();
    let info = LibraryInfo {
        uuid: Uuid::from_u128(10),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_ui::engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key":"libraries.paths","value":[lib]}),
        )
        .unwrap();
    let document: Uuid = shell
        .run(
            "library.element.new",
            json!({"library":info.uuid,"kind":kind,"name":"Test"}),
        )
        .unwrap()["element"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    if kind == "footprint" {
        shell
            .run(
                "selection.filter.set",
                json!({"document":document,"kinds":{"text":false}}),
            )
            .unwrap();
    }
    let mut graphics = Vec::new();
    for extent in ["1mm", "5mm"] {
        let mut params = json!({kind:document,"shape":{"kind":"rect","start":{"x":format!("-{extent}"),"y":format!("-{extent}")},"end":{"x":extent,"y":extent}}});
        if kind == "footprint" {
            params["layer"] = json!("silkscreen_top");
        }
        let command = format!("library.{kind}.graphic.add");
        graphics.push(
            shell.run(&command, params).unwrap()["result"]["graphic"]
                .as_str()
                .unwrap()
                .parse::<Uuid>()
                .unwrap(),
        );
    }
    let mut h = builder
        .with_size([1100.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    // A fixed view with room around the larger rectangle, whatever the canvas's size.
    h.state_mut()
        .shell_mut()
        .document_tab_mut(document)
        .unwrap()
        .camera = Some(reda_ui::canvas::Camera {
        center: (0.0, 0.0),
        nm_per_point: 30_000.0,
    });
    Canvas {
        h,
        document,
        small: graphics[0],
        large: graphics[1],
        _dir: dir,
    }
}

fn screen(c: &Canvas, x: f64, y: f64) -> Pos2 {
    let rect = c.h.get_by_role(Role::Canvas).rect();
    c.h.state()
        .shell()
        .document_tab(c.document)
        .unwrap()
        .camera
        .unwrap()
        .screen_point(rect, (x * 1e6, y * 1e6))
}

fn click(c: &mut Canvas, at: Pos2, modifiers: Modifiers) {
    c.h.event(egui::Event::ModifiersChanged(modifiers));
    c.h.drag_at(at);
    c.h.drop_at(at);
    c.h.run();
    c.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
    c.h.run();
}

#[test]
fn both_library_canvases_share_click_toggle_smallest_stack_and_filter_rules() {
    for kind in ["symbol", "footprint"] {
        let mut c = canvas(kind);
        c.h.run();
        let at = screen(&c, 0.0, 0.0);
        click(&mut c, at, Modifiers::NONE);
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.small],
            "{kind}: smallest object wins"
        );
        click(&mut c, at, Modifiers::SHIFT);
        assert!(
            c.h.state().shell().selection().is_empty(),
            "{kind}: Shift toggles out"
        );
        let ids = [c.small, c.large];
        run(&mut c.h, "selection.set", json!({"objects":ids}));
        click(&mut c, at, Modifiers::COMMAND);
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.small],
            "{kind}: Mod replaces a multi-selection"
        );
        c.h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
        c.h.run();
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.large],
            "{kind}: Shift+Tab cycles"
        );
        run(
            &mut c.h,
            "selection.filter.set",
            json!({"document":c.document,"kinds":{"graphic":false}}),
        );
        run(&mut c.h, "selection.set", json!({}));
        click(&mut c, at, Modifiers::NONE);
        assert!(
            c.h.state().shell().selection().is_empty(),
            "{kind}: filtered click"
        );
        c.h.key_press_modifiers(Modifiers::COMMAND, Key::A);
        c.h.run();
        assert!(
            c.h.state().shell().selection().is_empty(),
            "{kind}: filtered Select All"
        );
        let from = screen(&c, -6.0, -6.0);
        let to = screen(&c, 6.0, 6.0);
        c.h.drag_at(from);
        c.h.run();
        c.h.hover_at(to);
        c.h.run();
        c.h.drop_at(to);
        c.h.run();
        assert!(
            c.h.state().shell().selection().is_empty(),
            "{kind}: filtered rectangle"
        );
        run(&mut c.h, "selection.set", json!({"objects":[c.small]}));
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.small],
            "{kind}: explicit selection bypasses the Filter"
        );
    }
}

#[test]
fn footprint_editor_prefers_active_layer_pads_over_smaller_inactive_graphics() {
    let mut c = canvas("footprint");
    let pad:Uuid = run(&mut c.h,"library.footprint.pad.add",json!({"footprint":c.document,"number":"1","kind":"smd","shape":"rect","at":{"x":0,"y":0},"width":"8mm","height":"8mm"}))["result"]["pad"].as_str().unwrap().parse().unwrap();
    c.h.run();
    let at = screen(&c, 0.0, 0.0);
    click(&mut c, at, Modifiers::NONE);
    assert_eq!(
        c.h.state().shell().selection().objects,
        [pad],
        "the active copper Layer wins before size ordering"
    );
}

#[test]
fn library_drags_preserve_filtered_graphics_in_the_selection_and_in_place() {
    use reda_ui::engine::model::LibraryElement;
    for kind in ["symbol", "footprint"] {
        let mut c = canvas(kind);
        let (command, params, key) = if kind == "symbol" {
            (
                "library.symbol.pin.add",
                json!({"symbol":c.document,"number":"1","name":"A","at":{"x":0,"y":0}}),
                "pin",
            )
        } else {
            (
                "library.footprint.pad.add",
                json!({"footprint":c.document,"number":"1","kind":"smd","shape":"rect","at":{"x":0,"y":0},"width":"2mm","height":"2mm"}),
                "pad",
            )
        };
        let movable: Uuid = run(&mut c.h, command, params)["result"][key]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let before =
            c.h.state()
                .shell()
                .engine()
                .element(c.document)
                .unwrap()
                .clone();
        run(
            &mut c.h,
            "selection.set",
            json!({"objects":[movable,c.small]}),
        );
        run(
            &mut c.h,
            "selection.filter.set",
            json!({"document":c.document,"kinds":{"graphic":false}}),
        );
        c.h.run();
        let at = screen(&c, 0.0, 0.0);
        c.h.drag_at(at);
        c.h.run();
        c.h.hover_at(at + egui::vec2(50.0, 0.0));
        c.h.run();
        c.h.drop_at(at + egui::vec2(50.0, 0.0));
        c.h.run();
        assert_eq!(
            c.h.state().shell().selection().objects,
            [movable, c.small],
            "{kind}: filtered selection remains"
        );
        match (
            &before,
            c.h.state().shell().engine().element(c.document).unwrap(),
        ) {
            (LibraryElement::Symbol(old), LibraryElement::Symbol(new)) => {
                assert_eq!(old.graphics, new.graphics);
                assert!(new.pins[0].at.x > old.pins[0].at.x);
            }
            (LibraryElement::Footprint(old), LibraryElement::Footprint(new)) => {
                assert_eq!(old.graphics, new.graphics);
                assert!(new.pads[0].at.x > old.pads[0].at.x);
            }
            _ => panic!("expected the same Library element kind"),
        }
    }
}

/// [`click`], `gap` seconds after the last input, so clicks can be timed against the
/// 600 ms repeat window and egui's double-click.
fn click_after(c: &mut Canvas, at: Pos2, modifiers: Modifiers, gap: f64) {
    c.h.input_mut().time = Some(c.h.ctx.input(|i| i.time) + gap);
    click(c, at, modifiers);
}

fn rectangle(c: &mut Canvas, from: Pos2, to: Pos2) -> Vec<Uuid> {
    run(&mut c.h, "selection.set", json!({}));
    c.h.input_mut().time = Some(c.h.ctx.input(|i| i.time) + 1.0);
    c.h.drag_at(from);
    c.h.run();
    c.h.hover_at(to);
    c.h.run();
    c.h.drop_at(to);
    c.h.run();
    c.h.state().shell().selection().objects.clone()
}

#[test]
fn both_library_canvases_add_with_shift_cycle_on_repeat_and_keep_or_clear_on_empty_clicks() {
    for kind in ["symbol", "footprint"] {
        let mut c = canvas(kind);
        c.h.run();
        let (centre, edge) = (screen(&c, 0.0, 0.0), screen(&c, 5.0, 0.0));
        click_after(&mut c, edge, Modifiers::NONE, 1.0);
        assert_eq!(c.h.state().shell().selection().objects, [c.large]);
        click_after(&mut c, centre, Modifiers::SHIFT, 1.0);
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.large, c.small],
            "{kind}: Shift+click adds"
        );
        click_after(&mut c, centre, Modifiers::NONE, 1.0);
        assert_eq!(c.h.state().shell().selection().objects, [c.small]);
        click_after(&mut c, centre, Modifiers::NONE, 0.45);
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.large],
            "{kind}: a repeat click within 600 ms cycles"
        );
        run(
            &mut c.h,
            "app.preferences.set",
            json!({"key":"editing.click_clears_selection","value":false}),
        );
        let empty = c.h.get_by_role(Role::Canvas).rect().left_bottom() + egui::vec2(30.0, -60.0);
        click_after(&mut c, empty, Modifiers::NONE, 1.0);
        assert_eq!(
            c.h.state().shell().selection().objects,
            [c.large],
            "{kind}: an empty click keeps the selection with Click Clears Selection off"
        );
        run(
            &mut c.h,
            "app.preferences.set",
            json!({"key":"editing.click_clears_selection","value":true}),
        );
        click_after(&mut c, empty, Modifiers::NONE, 1.0);
        assert!(
            c.h.state().shell().selection().is_empty(),
            "{kind}: and clears it when on"
        );
    }
}

#[test]
fn both_library_canvases_pick_by_rectangle_direction() {
    for kind in ["symbol", "footprint"] {
        let mut c = canvas(kind);
        c.h.run();
        // Over a corner of both rectangles, enclosing neither; the corners are outside both.
        let (left, right) = (screen(&c, -6.0, 0.5), screen(&c, 0.5, -6.0));
        assert!(
            rectangle(&mut c, left, right).is_empty(),
            "{kind}: left to right selects only what it encloses"
        );
        let touched = rectangle(&mut c, right, left);
        assert!(
            touched.contains(&c.small) && touched.contains(&c.large),
            "{kind}: right to left selects what it touches"
        );
        run(
            &mut c.h,
            "app.preferences.set",
            json!({"key":"editing.directional_rectangle","value":false}),
        );
        assert!(
            rectangle(&mut c, right, left).is_empty(),
            "{kind}: with Directional Rectangle off every rectangle encloses"
        );
    }
}

#[test]
fn both_library_canvases_open_the_pick_list_on_a_double_click() {
    for kind in ["symbol", "footprint"] {
        let mut c = quick_canvas(kind);
        c.h.run();
        let at = screen(&c, 0.0, 0.0);
        for _ in 0..2 {
            click(&mut c, at, Modifiers::NONE);
        }
        c.h.get_by_label("Pick Object");
        assert!(
            c.h.state().shell().selection().is_empty(),
            "{kind}: the list opens over the selection from before the double-click"
        );
        assert!(
            c.h.query_by_label("Graphic · 1").is_some()
                && c.h.query_by_label("Graphic · 2").is_some(),
            "{kind}: both stacked graphics are listed"
        );
        c.h.get_by_label("Graphic · 2").click();
        c.h.run();
        let picked = &c.h.state().shell().selection().objects;
        assert!(
            picked.len() == 1 && [c.small, c.large].contains(&picked[0]),
            "{kind}: choosing an entry selects it"
        );
        assert!(c.h.query_by_label("Pick Object").is_none());
    }
}
