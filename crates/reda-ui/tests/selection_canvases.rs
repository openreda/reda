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
    let h = Harness::builder()
        .with_size([1100.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
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
