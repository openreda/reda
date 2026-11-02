//! The window, driven through AccessKit with `egui_kittest`: the Start page, opening and
//! creating Projects, and image snapshots of both themes and the schematic canvas.
//!
//! Snapshots are drawn by Mesa's llvmpipe and compared on Linux only; elsewhere, or on
//! another adapter, they are skipped with a message (`REDA_PNG_SNAPSHOTS=1` makes skipping
//! a failure, as in CI). Run them locally with
//! `LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_PNG_SNAPSHOTS=1 cargo test -p reda-ui
//! --test app`, and add `UPDATE_SNAPSHOTS=1` to rewrite `tests/snapshots/*.png`.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, HarnessBuilder};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::{Engine, preferences, state};
use reda_ui::shell::Shell;
use serde_json::json;

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

/// Creates a Project named `name` in `folder`; returns its `.reda` file.
fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    std::fs::canonicalize(folder.join(name).join(format!("{name}.reda"))).unwrap()
}

fn set_recent(home: &Path, recent: &[&Path]) {
    state::update(&UserDirs::under(home), |s| {
        s.recent_projects = recent.iter().map(|p| p.to_path_buf()).collect();
    })
    .unwrap();
}

fn builder() -> HarnessBuilder<App> {
    Harness::builder().with_size([900.0, 560.0])
}

fn harness(home: &Path) -> Harness<'static, App> {
    let app = App::new(Shell::new(engine_in(home)));
    builder().build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

#[test]
fn the_start_page_greys_missing_projects_removes_and_opens_them() {
    let home = TempDir::new("app-home");
    let work = TempDir::new("app-work");
    let alpha = make_project(work.path(), "Alpha");
    let gone = work.path().join("Gone/Gone.reda");
    set_recent(home.path(), &[&alpha, &gone]);
    let mut h = harness(home.path());
    h.run();

    assert!(!h.get_by_label("Alpha").accesskit_node().is_disabled());
    assert!(h.get_by_label("Gone").accesskit_node().is_disabled());

    h.get_by_label("Remove Gone from recent Projects").click();
    h.run();
    assert!(h.query_by_label("Gone").is_none());
    let recent = state::read(&UserDirs::under(home.path())).recent_projects;
    assert_eq!(recent, std::slice::from_ref(&alpha));

    h.get_by_label("Alpha").click();
    h.run();
    // The Start page closed; the first sheet shows in a tab, drawn by the canvas.
    assert!(h.query_by_label("Start").is_none());
    assert!(h.query_by_label("Welcome to REDA").is_none());
    // Its tab, and its item in the Projects panel.
    assert_eq!(h.query_all_by_label("Sheet 1").count(), 2);
    h.get_by_role_and_label(Role::Canvas, "Schematic canvas: Sheet 1");
    assert_eq!(
        h.state().shell().engine().project().unwrap().props().name,
        "Alpha"
    );
}

#[test]
fn the_new_project_form_creates_and_opens_a_project() {
    let home = TempDir::new("app-home");
    let work = TempDir::new("app-work");
    let mut h = harness(home.path());
    h.run();

    // The toolbar's New Project button comes first; the Start page's last.
    h.get_all_by_label("New Project…").last().unwrap().click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Name")
        .set_text_value_accesskit("Blinky");
    h.get_by_role_and_label(Role::TextInput, "Location")
        .set_text_value_accesskit(&work.path().display().to_string());
    h.run();
    h.get_by_label("Create").click();
    h.run();

    assert!(work.path().join("Blinky/Blinky.reda").is_file());
    h.get_by_role_and_label(Role::Canvas, "Schematic canvas: Sheet 1");
    assert!(h.query_by_label("New Project").is_none(), "the form closed");
}

#[test]
fn the_new_project_dialog_takes_a_fab_preset_and_a_rectangle_board() {
    let home = TempDir::new("app-home");
    let work = TempDir::new("app-work");
    state::update(&UserDirs::under(home.path()), |s| {
        s.last_fab_preset = Some("JLCPCB".into());
    })
    .unwrap();
    let mut h = harness(home.path());
    h.run();

    h.get_all_by_label("New Project…").last().unwrap().click();
    h.run();
    // The last Fab Preset used is the default.
    let picked = |h: &Harness<'_, App>, label: &str| {
        h.get_by_role_and_label(Role::RadioButton, label)
            .accesskit_node()
            .toggled()
            == Some(egui::accesskit::Toggled::True)
    };
    assert!(picked(&h, "JLCPCB"));
    assert!(!picked(&h, "Generic"));
    h.get_by_label("The board rules and the Output Job can be changed later.");
    h.get_by_role_and_label(Role::TextInput, "Name")
        .set_text_value_accesskit("Sized");
    h.get_by_role_and_label(Role::TextInput, "Location")
        .set_text_value_accesskit(&work.path().display().to_string());
    // The board size is used only once Rectangle is picked.
    assert!(
        h.get_by_role_and_label(Role::TextInput, "Width (mm)")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label("Rectangle").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Width (mm)")
        .set_text_value_accesskit("50");
    h.run();
    h.get_by_label("Create").click();
    h.run();

    let project = h.state().shell().engine().project().unwrap();
    let jobs: Vec<_> = project.output_jobs().map(|j| j.name.clone()).collect();
    assert_eq!(jobs, ["JLCPCB"]);
    let outline = project.pcb().unwrap().outline().expect("a Board Outline");
    let xs: Vec<i64> = outline.vertices.iter().map(|v| v.x.0).collect();
    assert_eq!(xs.iter().max(), Some(&50_000_000));
    let ys: Vec<i64> = outline.vertices.iter().map(|v| v.y.0).collect();
    assert_eq!(ys.iter().max(), Some(&80_000_000));
}

#[test]
fn the_new_project_dialog_refuses_a_bad_board_size() {
    let home = TempDir::new("app-home");
    let mut h = harness(home.path());
    h.run();
    h.get_all_by_label("New Project…").last().unwrap().click();
    h.run();
    h.get_by_label("Rectangle").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Height (mm)")
        .set_text_value_accesskit("-3");
    h.run();
    assert!(h.get_by_label("Create").accesskit_node().is_disabled());
}

/// Why snapshots cannot be compared here, if they cannot.
fn snapshot_skip_reason() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("theme snapshots are compared on Linux only".into());
    }
    match reda_ui::engine::render::gpu::Headless::new() {
        Ok(h) if h.adapter().name.contains("llvmpipe") => None,
        Ok(h) => Some(format!(
            "the adapter is {:?}, not Mesa's llvmpipe that drew the snapshots",
            h.adapter().name
        )),
        Err(e) => Some(e.to_string()),
    }
}

/// A harness that renders through wgpu with the canvas installed, or `None` (after a
/// message) when snapshots cannot be compared on this machine.
fn snapshot_harness(home: &Path) -> Option<Harness<'static, App>> {
    snapshot_harness_on(engine_in(home))
}

/// [`snapshot_harness`] on `engine`.
fn snapshot_harness_on(engine: Engine) -> Option<Harness<'static, App>> {
    if let Some(why) = snapshot_skip_reason() {
        if std::env::var_os("REDA_PNG_SNAPSHOTS").is_some() {
            panic!("REDA_PNG_SNAPSHOTS is set but snapshots cannot run: {why}");
        }
        eprintln!("skipping snapshot: {why}");
        return None;
    }
    let render_state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    reda_ui::install_canvas(&render_state, 1);
    let app = App::new(Shell::new(engine));
    Some(
        builder()
            .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
                render_state,
            ))
            .build_ui_state(|ui, app: &mut App| app.ui(ui), app),
    )
}

fn set_theme(home: &Path, theme: &str) {
    preferences::set(&UserDirs::under(home), "general.theme", json!(theme)).unwrap();
}

/// A fixed folder for Projects that snapshots show, so their paths are the same on every
/// machine (the snapshots run on Linux only).
fn snapshot_folder(name: &str) -> PathBuf {
    let dir = PathBuf::from("/tmp/reda-ui-snapshots").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn start_page_snapshot(theme: &str) {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder(&format!("start-{theme}"));
    let alpha = make_project(&folder, "Alpha");
    let gone = folder.join("Gone/Gone.reda");
    set_recent(home.path(), &[&alpha, &gone]);
    set_theme(home.path(), theme);
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    h.snapshot(format!("start_page_{theme}"));
}

#[test]
fn snapshot_of_the_dark_theme() {
    start_page_snapshot("dark");
}

#[test]
fn snapshot_of_the_light_theme() {
    start_page_snapshot("light");
}

/// Keyboard focus is visible: after F6 a ring marks the focused tab, and Ctrl+Tab held
/// shows the tab switcher.
#[test]
fn snapshot_of_keyboard_focus_and_the_tab_switcher() {
    let home = TempDir::new("app-home");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    h.key_press(egui::Key::F6);
    h.run();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::CTRL));
    for pressed in [true, false] {
        h.event(egui::Event::Key {
            key: egui::Key::Tab,
            pressed,
            modifiers: egui::Modifiers::CTRL,
            repeat: false,
            physical_key: None,
        });
    }
    h.run();
    h.snapshot("keyboard_focus_dark");
}

#[test]
fn snapshot_of_a_schematic_sheet_on_the_canvas() {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder("canvas");
    let file = make_project(&folder, "Canvas");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    let shell = h.state_mut().shell_mut();
    shell.open_project(&file).unwrap();
    let sheet = shell.active_document().expect("a sheet tab").document;
    shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet,
                   "points": [{"x": "50mm", "y": "100mm"}, {"x": "150mm", "y": "100mm"},
                              {"x": "150mm", "y": "60mm"}]}),
        )
        .unwrap();
    h.run();
    h.snapshot("schematic_canvas_dark");
}

#[test]
fn snapshot_of_a_drag_in_flight() {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder("drag");
    let file = make_project(&folder, "Drag");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    let shell = h.state_mut().shell_mut();
    shell.open_project(&file).unwrap();
    let sheet = shell.active_document().expect("a sheet tab").document;
    shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet,
                   "points": [{"x": "50mm", "y": "100mm"}, {"x": "150mm", "y": "100mm"},
                              {"x": "150mm", "y": "60mm"}]}),
        )
        .unwrap();
    // The Properties panel would show the Wire's UUID, which differs run to run.
    shell
        .run("layout.tab.close", json!({"tab": "properties"}))
        .unwrap();
    h.run();
    // Grab the horizontal Wire and drag it up: it is drawn moved and selected, and the
    // Wire down from its end is drawn rubber-banded, with nothing committed yet.
    let rect = h.get_by_role(Role::Canvas).rect();
    let camera = h
        .state()
        .shell()
        .document_tab(sheet)
        .unwrap()
        .camera
        .unwrap();
    let grab = camera.screen_point(rect, (100e6, 100e6));
    h.hover_at(grab);
    h.run();
    h.drag_at(grab);
    h.run();
    for step in 1..=4 {
        h.hover_at(grab + egui::vec2(0.0, -10.0 * step as f32));
        h.run();
    }
    assert!(
        h.state()
            .shell()
            .document_tab(sheet)
            .unwrap()
            .moving_offset
            .1
            > 0.0
    );
    h.snapshot("schematic_drag_in_flight_dark");
}

#[test]
fn snapshot_of_the_library_panel_placing_a_part() {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder("library");
    let file = make_project(&folder, "Library");
    set_theme(home.path(), "dark");
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(starter))
        .headless(false)
        .build();
    let Some(mut h) = snapshot_harness_on(engine) else {
        return;
    };
    h.run();
    let shell = h.state_mut().shell_mut();
    shell.open_project(&file).unwrap();
    let sheet = shell.active_document().expect("a sheet tab").document;
    shell
        .run("panel.show", json!({"panel": "library"}))
        .unwrap();
    h.run();
    // Choose the dual op-amp (Units as tabs, a Footprint) and hover over the sheet: the
    // Part follows the pointer, drawn but not placed.
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Search Parts")
        .type_text("OpAmp_Dual");
    h.run();
    // The first click takes the focus from the search box, which settles the panel's
    // layout at this narrow width; the second lands on the row.
    for _ in 0..2 {
        h.get_by_role_and_label(Role::ListItem, "OpAmp_Dual")
            .click();
        h.run();
    }
    assert!(reda_ui::placement::placing(h.state().shell()).is_some());
    let rect = h.get_by_role(Role::Canvas).rect();
    h.hover_at(rect.center());
    h.run();
    h.hover_at(rect.center() + egui::vec2(4.0, 2.0));
    h.run();
    let units = h
        .state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(sheet)
        .unwrap()
        .units()
        .count();
    assert_eq!(units, 0);
    h.snapshot("library_placing_dark");
}

/// A board with no Board Outline: Free Pads, a Track and a Via, one Pad selected, with
/// the Layers panel open beside it and the outline hint above.
#[test]
fn snapshot_of_a_board_and_the_layers_panel() {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder("board");
    let file = make_project(&folder, "Board");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    let shell = h.state_mut().shell_mut();
    shell.open_project(&file).unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let (pcb, top) = (pcb.uuid, pcb.copper_layers().next().unwrap().uuid);
    let mut pads = Vec::new();
    for x in ["0mm", "6mm"] {
        let out = shell
            .run(
                "pcb.free_pad.add",
                json!({"kind": "smd", "at": {"x": x, "y": 0}, "width": "2mm", "height": "1.2mm"}),
            )
            .unwrap();
        pads.push(out["result"]["uuid"].clone());
    }
    shell
        .run(
            "pcb.track.add",
            json!({"layer": top, "points": [{"x": "6mm", "y": 0}, {"x": "6mm", "y": "5mm"},
                                            {"x": "12mm", "y": "5mm"}], "width": "0.3mm"}),
        )
        .unwrap();
    shell
        .run("pcb.via.add", json!({"at": {"x": "12mm", "y": "5mm"}}))
        .unwrap();
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell.run("panel.show", json!({"panel": "layers"})).unwrap();
    shell
        .run("selection.set", json!({"objects": [pads[0]]}))
        .unwrap();
    h.run();
    h.snapshot("board_layers_dark");
}

/// A sheet of icons at the toolbar, menu and tab sizes, each row enabled then disabled.
fn icon_sheet_snapshot(theme: egui::Theme) {
    if let Some(why) = snapshot_skip_reason() {
        if std::env::var_os("REDA_PNG_SNAPSHOTS").is_some() {
            panic!("REDA_PNG_SNAPSHOTS is set but snapshots cannot run: {why}");
        }
        eprintln!("skipping snapshot: {why}");
        return;
    }
    let render_state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let keys = [
        "wire",
        "net_label",
        "power_port",
        "track",
        "via",
        "zone",
        "save",
        "undo",
        "zoom_fit",
        "panel_layers",
        "move",
        "erc",
    ];
    let mut h = Harness::builder()
        .with_size([400.0, 172.0])
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ))
        .build_ui(move |ui| {
            reda_ui::theme::install(ui.ctx(), theme.into());
            reda_ui::icons::install_loaders(ui.ctx());
            egui::Frame::central_panel(ui.style()).show(ui, |ui| {
                for size in [
                    reda_ui::icons::TOOLBAR,
                    reda_ui::icons::MENU,
                    reda_ui::icons::TAB,
                ] {
                    for enabled in [true, false] {
                        ui.horizontal(|ui| {
                            for key in keys {
                                ui.add(reda_ui::icons::image(key, size, theme, enabled).unwrap());
                            }
                        });
                    }
                }
            });
        });
    h.run();
    let name = match theme {
        egui::Theme::Dark => "icons_dark",
        egui::Theme::Light => "icons_light",
    };
    h.snapshot(name);
}

#[test]
fn snapshot_of_the_icons_on_the_dark_theme() {
    icon_sheet_snapshot(egui::Theme::Dark);
}

#[test]
fn snapshot_of_the_icons_on_the_light_theme() {
    icon_sheet_snapshot(egui::Theme::Light);
}

/// Drags the tab `tab` over the centre group and holds it on the drop target `target`.
fn drag_snapshot(tab: &str, target: &str, name: &str) {
    let home = TempDir::new("app-home");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    let from = h.get_by_label(tab).rect().center();
    let over = h.get_by_label("Welcome to REDA").rect().center();
    h.drag_at(from);
    h.run();
    h.hover_at(over);
    h.run();
    let at = h.get_by_label(target).rect().center();
    h.hover_at(at);
    h.run();
    h.snapshot(name);
}

#[test]
fn snapshot_of_a_tab_drag_previewing_a_split() {
    drag_snapshot("Properties", "Drop to the Left", "drag_preview_dark");
}

#[test]
fn snapshot_of_a_tab_drag_the_layout_would_refuse() {
    // The Start page is alone in its group, so it cannot split from it.
    drag_snapshot("Start", "Drop Below", "drag_refused_dark");
}

/// Project Settings on `page`, with a Project whose board thickness is typed wrong so the
/// flag shows.
fn project_settings_snapshot(page: &str, name: &str) {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder(name);
    let file = make_project(&folder, "Settings");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    h.state_mut().shell_mut().open_project(&file).unwrap();
    h.state_mut().open_project_settings();
    h.run();
    let field = h.get_by_role_and_label(Role::TextInput, "Board thickness (mm)");
    field.click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Board thickness (mm)")
        .type_text("1.6x");
    h.run();
    if page != "Properties" {
        h.get_by_role_and_label(Role::TreeItem, page).click();
    }
    h.run();
    h.snapshot(name);
}

#[test]
fn snapshot_of_project_settings_with_an_invalid_value() {
    project_settings_snapshot("Properties", "project_settings_properties_dark");
}

#[test]
fn snapshot_of_project_settings_erc_with_the_pin_matrix() {
    project_settings_snapshot("ERC", "project_settings_erc_dark");
}

/// Opens the Command Palette with `query` typed into it.
fn palette_with(h: &mut Harness<'static, App>, query: &str) {
    h.run();
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::P,
    );
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Command Palette")
        .type_text(query);
    h.run();
}

#[test]
fn snapshot_of_the_command_palette() {
    let home = TempDir::new("app-home");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    palette_with(&mut h, "show");
    h.snapshot("command_palette_dark");
}

#[test]
fn snapshot_of_a_generated_params_form() {
    let home = TempDir::new("app-home");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    palette_with(&mut h, "split tab");
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Split Tab").is_some());
    h.snapshot("params_form_dark");
}

#[test]
fn snapshot_of_the_footprint_editor() {
    let home = TempDir::new("app-home");
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    let library = "000000aa-0000-4000-8000-000000000000";
    let info = reda_ui::engine::model::LibraryInfo {
        uuid: library.parse().unwrap(),
        name: "Mine".into(),
        description: String::new(),
    };
    std::fs::write(
        lib.join("Mine.redalib"),
        reda_ui::engine::format::library::write_library_info(&info).unwrap(),
    )
    .unwrap();
    let dirs = UserDirs::under(home.path());
    preferences::set(&dirs, "libraries.paths", json!([lib])).unwrap();
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    let shell = h.state_mut().shell_mut();
    let out = shell
        .run(
            "library.element.new",
            json!({"library": library, "kind": "footprint", "name": "SOT-23-ish"}),
        )
        .unwrap();
    let fp = out["element"].clone();
    let mut pads = Vec::new();
    for (number, x, y) in [
        ("1", "-1mm", "-1.1mm"),
        ("2", "1mm", "-1.1mm"),
        ("3", "0mm", "1.1mm"),
    ] {
        let out = shell
            .run(
                "library.footprint.pad.add",
                json!({"footprint": fp, "number": number, "kind": "smd",
                       "shape": "rounded_rect", "at": {"x": x, "y": y},
                       "width": "0.6mm", "height": "0.9mm"}),
            )
            .unwrap();
        pads.push(out["result"]["pad"].clone());
    }
    shell
        .run(
            "library.footprint.pad.add",
            json!({"footprint": fp, "number": "4", "kind": "plated_through_hole",
                   "shape": "oval", "at": {"x": "3mm", "y": 0}, "width": "1.2mm",
                   "height": "2mm", "hole": {"width": "0.6mm", "height": "1.2mm"}}),
        )
        .unwrap();
    shell
        .run(
            "library.footprint.graphic.add",
            json!({"footprint": fp, "layer": "courtyard_top",
                   "shape": {"kind": "rect", "start": {"x": "-2mm", "y": "-2mm"},
                             "end": {"x": "4mm", "y": "2mm"}}}),
        )
        .unwrap();
    shell
        .run(
            "library.footprint.text.set",
            json!({"footprint": fp, "text": "designator", "at": {"x": 0, "y": "3mm"}}),
        )
        .unwrap();
    shell
        .run(
            "library.footprint.text.set",
            json!({"footprint": fp, "text": "value", "at": {"x": 0, "y": "-3mm"},
                   "size": "0.6mm"}),
        )
        .unwrap();
    shell
        .run("selection.set", json!({"objects": [pads[0]]}))
        .unwrap();
    h.run();
    h.run();
    h.snapshot("footprint_editor_dark");
}

/// Board Setup on `page`, on a board whose clearance differs from its Fab Preset, with
/// the Default class's clearance override typed wrong so the flag shows.
fn board_setup_snapshot(page: &str, name: &str) {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder(name);
    let file = make_project(&folder, "Board");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    let shell = h.state_mut().shell_mut();
    shell.open_project(&file).unwrap();
    shell
        .run("pcb.rules.set", json!({"clearance": "0.3mm"}))
        .unwrap();
    h.state_mut().open_board_setup();
    h.run();
    let field = "Default: Clearance";
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("-1");
    h.run();
    h.get_by_label("Apply Generic Rules…").click();
    h.run();
    if page != "Clearance" {
        h.get_by_role_and_label(Role::TreeItem, page).click();
    }
    h.run();
    h.snapshot(name);
}

#[test]
fn snapshot_of_board_setup_clearance_with_a_fab_preset_preview() {
    board_setup_snapshot("Clearance", "board_setup_clearance_dark");
}

#[test]
fn snapshot_of_board_setup_drc_severities() {
    board_setup_snapshot("DRC", "board_setup_drc_dark");
}

/// A harness on a Project with no Board Outline, so its job's run is blocked.
fn output_job_harness(name: &str) -> Option<(TempDir, Harness<'static, App>)> {
    let home = TempDir::new("app-home");
    let folder = snapshot_folder(name);
    let file = make_project(&folder, "Jobs");
    set_theme(home.path(), "dark");
    let mut h = snapshot_harness(home.path())?;
    h.run();
    h.state_mut().shell_mut().open_project(&file).unwrap();
    h.run();
    Some((home, h))
}

fn first_job(h: &Harness<'static, App>) -> reda_ui::engine::model::Uuid {
    let project = h.state().shell().engine().project().unwrap();
    project.output_jobs().next().unwrap().uuid
}

#[test]
fn snapshot_of_the_output_job_dialog_with_a_flagged_destination() {
    let Some((_home, mut h)) = output_job_harness("output_job_dialog_dark") else {
        return;
    };
    let job = first_job(&h);
    h.state_mut().open_output_job(job);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Destination folder")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Destination folder")
        .type_text("../fab");
    h.run();
    h.snapshot("output_job_dialog_dark");
}

#[test]
fn snapshot_of_a_blocked_run_report() {
    let Some((_home, mut h)) = output_job_harness("output_job_report_dark") else {
        return;
    };
    let job = first_job(&h);
    h.state_mut().open_output_job(job);
    h.run();
    h.get_by_role_and_label(Role::Button, "Run").click();
    let start = std::time::Instant::now();
    while h.query_by_label_contains("did not run").is_none() {
        assert!(start.elapsed().as_secs() < 20, "the run never ended");
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.step();
    }
    h.run();
    h.snapshot("output_job_report_dark");
}

/// The Preferences dialog on `page`, in the dark theme.
fn preferences_snapshot(page: Option<reda_ui::preferences_dialog::PageId>, name: &str) {
    let home = TempDir::new("app-home");
    set_theme(home.path(), "dark");
    let Some(mut h) = snapshot_harness(home.path()) else {
        return;
    };
    h.run();
    h.state_mut().open_preferences(page);
    h.run();
    h.snapshot(name);
}

#[test]
fn snapshot_of_the_preferences_general_page() {
    preferences_snapshot(None, "preferences_general_dark");
}

#[test]
fn snapshot_of_the_keyboard_shortcuts_page() {
    preferences_snapshot(
        Some(reda_ui::preferences_dialog::PageId::KeyboardShortcuts),
        "preferences_keyboard_shortcuts_dark",
    );
}
