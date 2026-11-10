//! The Layer control on the PCB canvas (a swatch at the top right, or a strip along the
//! bottom, as a Preference), Layer sets with Save current as, the Layers panel's group
//! eyes and set buttons, and the live hover outline, driven through AccessKit with
//! `egui_kittest`.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Modifiers, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Point;
use reda_ui::engine::model::{LayerKind, Side, Uuid};
use reda_ui::shell::Shell;
use serde_json::json;

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    gnd: Uuid,
    _home: TempDir,
    _work: TempDir,
}

/// The window on a board with two Footprints, each with one 2 mm Pad on GND, at x = 0
/// and x = 10 mm, with its PCB tab focused and the Layers panel open.
fn window() -> Window {
    let home = TempDir::new("layer-control-home");
    let work = TempDir::new("layer-control-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let gnd: Uuid = shell.run("test.net.add", json!({"name": "GND"})).unwrap()["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "test.footprint.add",
                json!({"at": {"x": x, "y": 0}, "pads": [
                    {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm", "net": gnd}
                ]}),
            )
            .unwrap();
    }
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    shell.run("panel.show", json!({"panel": "layers"})).unwrap();
    shell
        .run("view.zoom_fit", json!({"document": pcb}))
        .unwrap();
    let h = Harness::builder()
        .with_step_dt(0.25)
        .with_max_steps(100)
        .with_size([1000.0, 1100.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    Window {
        h,
        pcb,
        gnd,
        _home: home,
        _work: work,
    }
}

fn shell(w: &Window) -> &Shell {
    w.h.state().shell()
}

fn last_command(w: &Window) -> String {
    shell(w).engine().journal().last().unwrap().id.clone()
}

fn layer(w: &Window, kind: LayerKind, side: Option<Side>) -> Uuid {
    let pcb = shell(w).engine().project().unwrap().pcb().unwrap();
    pcb.layers().find(kind, side).unwrap().uuid
}

fn set_pref(w: &mut Window, key: &str, value: &str) {
    w.h.state_mut()
        .shell_mut()
        .run("app.preferences.set", json!({"key": key, "value": value}))
        .unwrap();
    w.h.run();
}

fn has_button(w: &Window, label: &str) -> bool {
    w.h.get_all_by_role(Role::Button)
        .any(|n| n.accesskit_node().label().as_deref() == Some(label))
}

fn button<'w>(w: &'w Window, label: &str) -> egui_kittest::Node<'w> {
    w.h.get_all_by_role(Role::Button)
        .find(|n| n.accesskit_node().label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no button {label}"))
}

/// Where the board point `p` shows in the window.
fn on_screen(w: &Window, p: Point) -> Pos2 {
    let canvas = w.h.get_by_role(Role::Canvas);
    let rect = canvas.rect();
    let camera = shell(w).document_tab(w.pcb).unwrap().camera.unwrap();
    camera.screen_point(rect, (p.x.0 as f64, p.y.0 as f64))
}

#[test]
fn the_swatch_shows_the_active_layer_and_its_chips_activate_and_mod_click_highlights() {
    let mut w = window();
    w.h.run();
    let canvas = w.h.get_by_role(Role::Canvas).rect();
    let swatch = w.h.get_by_label("Active Layer: Top Copper");
    let at = swatch.rect();
    assert!(
        at.center().x > canvas.center().x && at.center().y < canvas.center().y,
        "the swatch is at the canvas's top right"
    );
    assert!(!has_button(&w, "Layer sets"), "no strip by default");
    swatch.click();
    w.h.run();

    // The dropdown lists the Layers copper first, each as a chip.
    let chips: Vec<String> =
        w.h.get_all_by_role(Role::Button)
            .filter_map(|n| n.accesskit_node().label())
            .filter_map(|l| l.strip_prefix("Layer: ").map(str::to_owned))
            .collect();
    assert_eq!(chips[..2], ["Top Copper", "Bottom Copper"]);
    assert!(chips.contains(&"Board Outline".to_owned()));

    w.h.get_by_label("Layer: Bottom Copper").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.activate");
    let bottom = layer(&w, LayerKind::Copper, Some(Side::Bottom));
    assert_eq!(shell(&w).board_view().layers.active, Some(bottom));
    assert!(w.h.query_by_label("Active Layer: Bottom Copper").is_some());

    w.h.get_by_label("Active Layer: Bottom Copper").click();
    w.h.run();
    w.h.get_by_label("Layer: Top Silkscreen")
        .click_modifiers(Modifiers::COMMAND);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.highlight");
    let silk = layer(&w, LayerKind::Silkscreen, Some(Side::Top));
    let view = shell(&w).board_view();
    assert_eq!(view.layers.highlighted, Some(silk));
    assert_eq!(
        view.layers.active,
        Some(bottom),
        "Mod+click does not activate"
    );
}

#[test]
fn the_strip_runs_along_the_bottom_with_a_chip_per_shown_layer_and_layer_sets() {
    let mut w = window();
    w.h.run();
    let readout_before = w.h.get_by_label("Grid readout").rect();
    set_pref(&mut w, "pcb.view.layer_control", "strip");
    w.h.run();
    assert!(
        w.h.query_by_label("Active Layer: Top Copper").is_none(),
        "the strip replaces the swatch"
    );
    let canvas = w.h.get_by_role(Role::Canvas).rect();
    let sets = button(&w, "Layer sets").rect();
    assert!(sets.bottom() <= canvas.bottom() && sets.top() >= canvas.bottom() - 24.0);

    // A chip per shown Layer, copper first, in one row at the bottom.
    let chips = |w: &Window| -> Vec<(String, f32)> {
        w.h.get_all_by_role(Role::Button)
            .filter_map(|n| {
                let label = n.accesskit_node().label()?;
                Some((
                    label.strip_prefix("Layer: ")?.to_owned(),
                    n.rect().center().y,
                ))
            })
            .collect()
    };
    let shown = chips(&w);
    assert_eq!(shown[0].0, "Top Copper");
    assert_eq!(shown[1].0, "Bottom Copper");
    assert!(shown.iter().all(|(_, y)| *y > canvas.bottom() - 24.0));

    // The grid readout moves up 24 px to clear it.
    let readout = w.h.get_by_label("Grid readout").rect();
    assert_eq!(readout_before.bottom() - readout.bottom(), 24.0);

    // Layer sets from the strip: Signal hides everything but copper.
    button(&w, "Layer sets").click();
    w.h.run();
    w.h.get_all_by_label("Signal").last().unwrap().click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.view.layer_set.apply");
    let names: Vec<String> = chips(&w).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["Top Copper", "Bottom Copper"]);

    w.h.get_by_label("Layer: Bottom Copper").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.activate");
    w.h.get_by_label("Layer: Top Copper")
        .click_modifiers(Modifiers::COMMAND);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.highlight");
    let top = layer(&w, LayerKind::Copper, Some(Side::Top));
    assert_eq!(shell(&w).board_view().layers.highlighted, Some(top));
}

#[test]
fn tool_bars_sit_36_px_above_the_canvas_bottom_and_24_px_more_with_the_strip() {
    use reda_ui::bars::draw::{grid_readout_anchor, tool_bar_anchor};
    let canvas = egui::Rect::from_min_max(egui::pos2(100.0, 50.0), egui::pos2(900.0, 650.0));
    assert_eq!(tool_bar_anchor(canvas, false), egui::pos2(500.0, 614.0));
    assert_eq!(tool_bar_anchor(canvas, true), egui::pos2(500.0, 590.0));
    assert_eq!(grid_readout_anchor(canvas, false), egui::pos2(108.0, 642.0));
    assert_eq!(grid_readout_anchor(canvas, true), egui::pos2(108.0, 618.0));
}

/// Types `name` into the open Save Layer Set dialog and saves it.
fn save_set_as(w: &mut Window, name: &str) {
    let field = w.h.get_by_role(Role::TextInput);
    field.click();
    w.h.run();
    w.h.get_by_role(Role::TextInput).type_text(name);
    w.h.run();
    w.h.get_all_by_role(Role::Button)
        .rfind(|n| n.accesskit_node().label().as_deref() == Some("Run"))
        .unwrap()
        .click();
    w.h.run();
}

#[test]
fn save_current_as_saves_the_shown_layers_from_the_swatch_and_the_panel() {
    let mut w = window();
    w.h.run();
    let bottom_silk = layer(&w, LayerKind::Silkscreen, Some(Side::Bottom));
    w.h.state_mut()
        .shell_mut()
        .run(
            "pcb.layer.show",
            json!({"layer": bottom_silk, "visible": false}),
        )
        .unwrap();
    w.h.get_by_label("Active Layer: Top Copper").click();
    w.h.run();
    w.h.get_all_by_label("Save current as…")
        .last()
        .unwrap()
        .click();
    w.h.run();
    save_set_as(&mut w, "NoBottomSilk");
    assert_eq!(last_command(&w), "pcb.view.layer_set.save");
    let saved = &shell(&w).preferences().pcb.view.layer_sets;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].name, "NoBottomSilk");
    assert!(!saved[0].layers.contains(&"silkscreen_bottom".to_owned()));
    assert!(saved[0].layers.contains(&"copper_top".to_owned()));

    // The saved set gets its own button in the panel, which applies it.
    w.h.state_mut()
        .shell_mut()
        .run("pcb.view.layer_set.apply", json!({"set": "all"}))
        .unwrap();
    w.h.run();
    assert!(shell(&w).board_view().layers.hidden.is_empty());
    w.h.get_by_label("NoBottomSilk").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.view.layer_set.apply");
    let hidden = &shell(&w).board_view().layers.hidden;
    assert_eq!(hidden.iter().copied().collect::<Vec<_>>(), [bottom_silk]);

    // The panel's own Save current as button.
    w.h.get_all_by_label("Save current as…")
        .next()
        .unwrap()
        .click();
    w.h.run();
    save_set_as(&mut w, "Second");
    let saved = &shell(&w).preferences().pcb.view.layer_sets;
    assert_eq!(saved.len(), 2);
}

#[test]
fn the_layers_panel_has_a_group_eye_per_kind_and_set_buttons_and_mod_click_pins() {
    let mut w = window();
    w.h.run();
    let top = layer(&w, LayerKind::Copper, Some(Side::Top));
    let bottom = layer(&w, LayerKind::Copper, Some(Side::Bottom));
    for kind in ["Copper", "Silkscreen", "Solder Mask", "Board Outline"] {
        assert!(
            w.h.query_by_label(&format!("Show all {kind} Layers"))
                .is_some(),
            "a group eye for {kind}"
        );
    }
    w.h.get_by_label("Show all Copper Layers").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.show_kind");
    let hidden = &shell(&w).board_view().layers.hidden;
    assert!(hidden.contains(&top) && hidden.contains(&bottom));
    assert_eq!(hidden.len(), 2, "only copper");
    w.h.get_by_label("Show all Copper Layers").click();
    w.h.run();
    assert!(shell(&w).board_view().layers.hidden.is_empty());

    // The set buttons apply a set in one click.
    w.h.get_by_label("Top").click();
    w.h.run();
    assert_eq!(last_command(&w), "pcb.view.layer_set.apply");
    assert!(shell(&w).board_view().layers.hidden.contains(&bottom));

    // Mod+click on an eye pins the Layer.
    w.h.get_by_label("Show Board Outline")
        .click_modifiers(Modifiers::COMMAND);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.pin");
    let outline = layer(&w, LayerKind::BoardOutline, None);
    assert!(shell(&w).board_view().layers.pinned.contains(&outline));

    // Mod+click on a Layer's name highlights its content, as on a chip.
    w.h.get_by_label("Top Copper")
        .click_modifiers(Modifiers::COMMAND);
    w.h.run();
    assert_eq!(last_command(&w), "pcb.layer.highlight");
    assert_eq!(shell(&w).board_view().layers.highlighted, Some(top));
}

#[test]
fn a_highlighted_layer_draws_on_top_at_full_strength_and_clear_ends_it() {
    use reda_ui::engine::render::SceneLayer;
    let mut w = window();
    w.h.run();
    let silk = layer(&w, LayerKind::Silkscreen, Some(Side::Bottom));
    let s = w.h.state_mut().shell_mut();
    s.run("pcb.layer.show", json!({"layer": silk, "visible": false}))
        .unwrap();
    s.run("pcb.layer.highlight", json!({"layer": silk}))
        .unwrap();
    let pcb = s.engine().project().unwrap().pcb().unwrap().clone();
    let view = s.board_view();
    assert!(!view.layers.hidden.contains(&silk), "highlighting shows it");
    let composites = view.composites(&pcb, Default::default());
    let board: Vec<_> = composites
        .iter()
        .filter(|c| matches!(c.layer, SceneLayer::Board(_)))
        .collect();
    let last = board.last().unwrap();
    assert_eq!(last.layer, SceneLayer::Board(silk));
    assert_eq!(last.opacity, 1.0);
    assert!(board[..board.len() - 1].iter().all(|c| c.opacity <= 0.15));

    s.run("view.highlight.clear", json!({})).unwrap();
    assert_eq!(s.board_view().layers.highlighted, None);
    // Toggling twice ends it too.
    s.run("pcb.layer.highlight", json!({"layer": silk}))
        .unwrap();
    s.run("pcb.layer.highlight", json!({"layer": silk}))
        .unwrap();
    assert_eq!(s.board_view().layers.highlighted, None);
}

#[test]
fn live_hover_outlines_the_net_under_the_cursor_off_on_or_with_shift_only() {
    let mut w = window();
    w.h.run();
    let pad = on_screen(&w, Point::default());
    let hover = |w: &mut Window, shift: bool| {
        let modifiers = if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        w.h.event(egui::Event::ModifiersChanged(modifiers));
        w.h.hover_at(pad + egui::vec2(1.0, 0.0));
        w.h.run();
        w.h.hover_at(pad);
        w.h.run();
        let shown = shell(w).live_hover_shown();
        w.h.event(egui::Event::ModifiersChanged(Modifiers::NONE));
        w.h.run();
        shown
    };
    // Shift only is the default.
    assert_eq!(
        shell(&w).preferences().pcb.view.live_hover,
        Default::default()
    );
    assert_eq!(hover(&mut w, false), None);
    assert_eq!(hover(&mut w, true), Some(w.gnd));
    set_pref(&mut w, "pcb.view.live_hover", "on");
    assert_eq!(hover(&mut w, false), Some(w.gnd));
    set_pref(&mut w, "pcb.view.live_hover", "off");
    assert_eq!(hover(&mut w, true), None);
    // Off the copper there is no Net to outline.
    set_pref(&mut w, "pcb.view.live_hover", "on");
    w.h.hover_at(on_screen(
        &w,
        Point::new(
            reda_ui::engine::model::geom::Nm(5_000_000),
            reda_ui::engine::model::geom::Nm(4_000_000),
        ),
    ));
    w.h.run();
    assert_eq!(shell(&w).live_hover_shown(), None);
}

#[test]
fn the_live_hover_outline_runs_round_each_copper_item_of_the_net() {
    use reda_ui::engine::model::geom::{Millideg, Nm};
    use reda_ui::engine::render::Primitive;
    use reda_ui::pcb::live_hover::{net_items, outline};
    let w = window();
    let shell = shell(&w);
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let scene = &shell.document_tab(w.pcb).unwrap().scene.scene;
    // Both Pads of GND, one per Footprint.
    let items = net_items(pcb, scene, w.gnd);
    assert_eq!(items.len(), 2);

    let p = |x: i64, y: i64| Point::new(Nm(x), Nm(y));
    // A 2 mm square Pad: its four corners, closed.
    let square = outline(&Primitive::Rect {
        center: p(0, 0),
        size: (Nm(2_000_000), Nm(2_000_000)),
        rotation: Millideg(0),
        corner_radius: Nm(0),
        stroke: None,
    });
    assert_eq!(
        square,
        [vec![
            p(-1_000_000, -1_000_000),
            p(1_000_000, -1_000_000),
            p(1_000_000, 1_000_000),
            p(-1_000_000, 1_000_000),
            p(-1_000_000, -1_000_000)
        ]]
    );
    // A Track from (0,0) to (10,0) mm, 1 mm wide: a stadium reaching 0.5 mm round it.
    let track = outline(&Primitive::Capsule {
        a: p(0, 0),
        b: p(10_000_000, 0),
        width: Nm(1_000_000),
    });
    let line = &track[0];
    assert_eq!(line.first(), line.last(), "closed");
    let xs: Vec<i64> = line.iter().map(|q| q.x.0).collect();
    let ys: Vec<i64> = line.iter().map(|q| q.y.0).collect();
    assert_eq!(*xs.iter().min().unwrap(), -500_000);
    assert_eq!(*xs.iter().max().unwrap(), 10_500_000);
    assert_eq!(
        (*ys.iter().min().unwrap(), *ys.iter().max().unwrap()),
        (-500_000, 500_000)
    );
    // A Via: its outer circle.
    let via = outline(&Primitive::Ring {
        center: p(0, 0),
        outer: Nm(300_000),
        inner: Nm(150_000),
    });
    assert!(via[0].iter().all(|q| {
        let r = ((q.x.0 as f64).powi(2) + (q.y.0 as f64).powi(2)).sqrt();
        (r - 300_000.0).abs() < 2.0
    }));
}
