//! Which Panel's selection is the live one: the Panel the user last pressed in or moved
//! the keyboard into, without the layout's focus (and so the undo stack and the menus)
//! moving to a Panel that does not take it.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::layout::{PanelId, TabKey};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn mil(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// A window on a Project with a Resistor on its sheet, a board with two Nets and their
/// Tracks, and ERC run: something to click in every Panel.
fn window(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(reda_testkit::starter_library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    let sheet = shell.engine().project().unwrap().props().documents[0];
    let parts = shell.run("library.search", json!({})).unwrap();
    let resistor = (parts["parts"].as_array().unwrap().iter())
        .find(|p| p["name"] == "Resistor")
        .map(|p| p["uuid"].clone())
        .unwrap();
    shell
        .run(
            "schematic.component.place",
            json!({"document": sheet, "part": resistor, "at": mil(2000, 2000)}),
        )
        .unwrap();
    for name in ["GND", "SIG"] {
        let out = shell.run("test.net.add", json!({ "name": name })).unwrap();
        let net = out["result"]["uuid"].clone();
        shell
            .run(
                "pcb.track.add",
                json!({"layer": "copper_top", "net": net, "width": "0.5mm",
                       "points": [{"x": "0mm", "y": "0mm"}, {"x": "4mm", "y": "0mm"}]}),
            )
            .unwrap();
    }
    let mut h = Harness::builder()
        .with_size([1400.0, 1000.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();
    h
}

fn shell<'h>(h: &'h Harness<'static, App>) -> &'h Shell {
    h.state().shell()
}

/// Shows `panel` over the sheet or the board, with the layout's focus on that Document.
fn show(h: &mut Harness<'static, App>, panel: &str, board: bool) -> TabKey {
    let shell = h.state_mut().shell_mut();
    let project = shell.engine().project().unwrap();
    let document = match board {
        true => project.pcb().unwrap().uuid,
        false => project.props().documents[0],
    };
    shell.run("panel.show", json!({ "panel": panel })).unwrap();
    shell
        .run("document.open", json!({ "document": document }))
        .unwrap();
    h.run();
    let focus = TabKey::Document(document);
    assert_eq!(h.state().shell().layout().focus, Some(focus.clone()));
    focus
}

/// The Panels in use among those that have rows.
fn in_use(h: &Harness<'static, App>) -> Vec<PanelId> {
    let panels = [
        PanelId::Projects,
        PanelId::Properties,
        PanelId::Messages,
        PanelId::Library,
        PanelId::Layers,
        PanelId::Navigator,
        PanelId::Find,
    ];
    (panels.into_iter())
        .filter(|p| shell(h).panel_focused(*p))
        .collect()
}

/// Clicks the Document's canvas, away from anything on it.
fn click_the_canvas(h: &mut Harness<'static, App>) {
    let canvas = h.query_all_by_role(Role::Canvas).next().unwrap().rect();
    let at = canvas.left_bottom() + egui::vec2(40.0, -40.0);
    h.hover_at(at);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
}

#[test]
fn with_nothing_pressed_the_layouts_focus_says_which_panel_is_in_use() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    show(&mut h, "layers", true);
    assert_eq!(in_use(&h), [], "a Document has the focus");
    let shell = h.state_mut().shell_mut();
    shell
        .run("layout.tab.activate", json!({"tab": "layers"}))
        .unwrap();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Layers]);
}

#[test]
fn a_press_on_a_layers_row_makes_layers_the_panel_in_use_and_leaves_the_focus() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    let focus = show(&mut h, "layers", true);
    h.get_by_label("Bottom Copper").scroll_to_me();
    h.run();
    h.get_by_label("Bottom Copper").click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Layers]);
    assert_eq!(shell(&h).layout().focus, Some(focus.clone()));
    // It lasts over later frames, and ends with a press on the Document.
    h.run();
    assert_eq!(in_use(&h), [PanelId::Layers]);
    click_the_canvas(&mut h);
    assert_eq!(in_use(&h), []);
    assert_eq!(shell(&h).layout().focus, Some(focus));
}

#[test]
fn a_press_on_a_navigator_row_makes_the_navigator_the_panel_in_use() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    let focus = show(&mut h, "navigator", true);
    h.get_by_label("Net GND").click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Navigator]);
    assert_eq!(shell(&h).layout().focus, Some(focus));
    click_the_canvas(&mut h);
    assert_eq!(in_use(&h), []);
}

#[test]
fn a_press_on_a_library_row_makes_the_library_the_panel_in_use() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    let focus = show(&mut h, "library", false);
    let row = h.query_all_by_role(Role::ListItem).next().unwrap();
    row.scroll_to_me();
    h.run();
    h.query_all_by_role(Role::ListItem).next().unwrap().click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Library]);
    assert_eq!(shell(&h).layout().focus, Some(focus));
}

#[test]
fn a_press_on_a_message_keeps_messages_in_use_though_it_goes_to_the_sheet() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    // The board has the focus; going to the Violation opens its sheet, which takes it.
    show(&mut h, "messages", true);
    h.get_by_label("Pin R1-1 is not connected").click();
    h.run();
    let sheet = shell(&h).engine().project().unwrap().props().documents[0];
    assert_eq!(shell(&h).layout().focus, Some(TabKey::Document(sheet)));
    assert_eq!(in_use(&h), [PanelId::Messages]);
    h.run();
    assert_eq!(in_use(&h), [PanelId::Messages]);
    click_the_canvas(&mut h);
    assert_eq!(in_use(&h), []);
}

#[test]
fn a_press_on_a_find_result_keeps_find_in_use_though_it_goes_to_the_sheet() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    show(&mut h, "find", true);
    let shell_mut = h.state_mut().shell_mut();
    reda_ui::find::edit(shell_mut, |p| p.text = "R".into());
    reda_ui::find::search(shell_mut);
    h.run();
    click_the_canvas(&mut h);
    assert_eq!(in_use(&h), []);
    h.get_by_label("R1 (Designator) on Sheet 1").click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Find]);
}

#[test]
fn a_press_in_another_panel_moves_the_use_there() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    show(&mut h, "navigator", true);
    show(&mut h, "layers", true);
    h.get_by_label("Net GND").click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Navigator]);
    h.get_by_label("Bottom Copper").scroll_to_me();
    h.run();
    h.get_by_label("Bottom Copper").click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Layers]);
    // A Panel that takes the layout's focus on a press is in use by that.
    show(&mut h, "projects", true);
    h.query_all_by_role(Role::TreeItem).next().unwrap().click();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Projects]);
    let projects = TabKey::Panel(PanelId::Projects);
    assert_eq!(shell(&h).layout().focus, Some(projects));
}

#[test]
fn moving_the_keyboard_into_a_panel_makes_it_the_panel_in_use() {
    let (home, work) = (TempDir::new("focus-home"), TempDir::new("focus-work"));
    let mut h = window(&home, &work);
    let focus = show(&mut h, "layers", true);
    click_the_canvas(&mut h);
    assert_eq!(in_use(&h), []);
    h.get_by_label("View from bottom").focus();
    h.run();
    assert_eq!(in_use(&h), [PanelId::Layers]);
    assert_eq!(shell(&h).layout().focus, Some(focus));
}
