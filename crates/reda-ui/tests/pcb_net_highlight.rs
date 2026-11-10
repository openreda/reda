//! Net highlight on the board (docs/spec/v0.1/editing.md#net-highlight): `Mod`+click
//! highlights a Net, Shift+`Mod`+click adds one, `Mod`+click on empty space clears, and a
//! `Mod`+drag still detaches; the `[modifiers]` table of `keymap.toml` can move the gesture.

use std::path::{Path, PathBuf};

use egui::Modifiers;
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const TOLERANCE: Nm = Nm::from_um(200);

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

struct Board {
    shell: Shell,
    /// Two Tracks on SIG, joined.
    sig: Vec<Uuid>,
    /// One Track on GND.
    gnd: Vec<Uuid>,
    _home: TempDir,
    _work: TempDir,
}

fn net(shell: &mut Shell, name: &str) -> Uuid {
    run(shell, "test.net.add", json!({"name": name}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

fn route(shell: &mut Shell, net: Uuid, points: &[(f64, f64)]) -> Vec<Uuid> {
    let points: Vec<Value> = points
        .iter()
        .map(|(x, y)| json!({"x": format!("{x}mm"), "y": format!("{y}mm")}))
        .collect();
    let out = run(
        shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "points": points, "net": net}),
    );
    out["result"]["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u.as_str().unwrap().parse().unwrap())
        .collect()
}

/// A board with SIG routed along y = 0 and GND along y = 5 mm; `keymap` is the user's
/// `keymap.toml`.
fn board(keymap: Option<&str>) -> Board {
    let home = TempDir::new("pcb-net-highlight-home");
    let work = TempDir::new("pcb-net-highlight-work");
    let dirs = UserDirs::under(home.path());
    if let Some(keymap) = keymap {
        std::fs::create_dir_all(&dirs.config).unwrap();
        std::fs::write(dirs.config.join("keymap.toml"), keymap).unwrap();
    }
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(dirs))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.reload_keymap();
    shell.new_project(work.path(), "Board").unwrap();
    let sig_net = net(&mut shell, "SIG");
    let gnd_net = net(&mut shell, "GND");
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    let sig = route(
        &mut shell,
        sig_net,
        &[(0.0, 0.0), (10.0, 0.0), (10.0, -5.0)],
    );
    let gnd = route(&mut shell, gnd_net, &[(0.0, 5.0), (10.0, 5.0)]);
    Board {
        shell,
        sig,
        gnd,
        _home: home,
        _work: work,
    }
}

fn click(b: &mut Board, at: Point, modifiers: Modifiers) {
    b.shell.pcb_press_with_modifiers(at, modifiers, TOLERANCE);
    b.shell.pcb_release();
}

fn highlighted(b: &Board) -> Vec<Uuid> {
    b.shell.highlight().objects.clone()
}

#[test]
fn mod_click_highlights_the_net_and_selects_nothing() {
    let mut b = board(None);
    let journal = b.shell.engine().journal().len();
    click(&mut b, mm(3.0, 0.0), Modifiers::COMMAND);
    let lit = highlighted(&b);
    for t in &b.sig {
        assert!(lit.contains(t), "the whole SIG Net");
    }
    assert!(!lit.contains(&b.gnd[0]));
    assert_eq!(b.shell.highlight().net.as_deref(), Some("SIG"));
    assert!(b.shell.selection().objects.is_empty());
    let ids: Vec<&str> = b.shell.engine().journal()[journal..]
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(ids, ["view.highlight.net_of"]);
}

#[test]
fn shift_mod_click_adds_a_net_and_mod_click_on_empty_space_clears() {
    let mut b = board(None);
    click(&mut b, mm(3.0, 0.0), Modifiers::COMMAND);
    click(&mut b, mm(3.0, 5.0), Modifiers::COMMAND | Modifiers::SHIFT);
    let lit = highlighted(&b);
    assert!(lit.contains(&b.sig[0]) && lit.contains(&b.gnd[0]));
    // A plain Mod+click replaces it.
    click(&mut b, mm(3.0, 5.0), Modifiers::COMMAND);
    assert!(!highlighted(&b).contains(&b.sig[0]));
    assert!(highlighted(&b).contains(&b.gnd[0]));
    click(&mut b, mm(30.0, 30.0), Modifiers::COMMAND);
    assert!(highlighted(&b).is_empty());
    assert!(
        b.shell.selection().objects.is_empty(),
        "the selection stays"
    );
}

#[test]
fn a_plain_click_still_selects_and_a_mod_drag_still_detaches() {
    let mut b = board(None);
    click(&mut b, mm(3.0, 0.0), Modifiers::NONE);
    assert_eq!(b.shell.selection().objects, [b.sig[0]]);
    assert!(highlighted(&b).is_empty());
    // Mod+drag on the GND segment: a drag Command, no highlight.
    b.shell
        .pcb_press_with_modifiers(mm(3.0, 5.0), Modifiers::COMMAND, TOLERANCE);
    b.shell.pcb_drag(mm(3.0, 6.0));
    b.shell.pcb_drag(mm(3.0, 7.0));
    b.shell.pcb_release();
    assert!(highlighted(&b).is_empty());
    let last = b.shell.engine().journal().last().unwrap();
    assert!(last.id.starts_with("pcb.track."), "{}", last.id);
    assert_eq!(last.params["detach"], json!(true), "{}", last.params);
}

#[test]
fn the_modifiers_table_rebinds_board_net_highlight() {
    let mut b = board(Some("[modifiers]\nnet_highlight_board = \"Alt\"\n"));
    click(&mut b, mm(3.0, 0.0), Modifiers::COMMAND);
    assert!(highlighted(&b).is_empty());
    assert_eq!(b.shell.selection().objects, [b.sig[0]]);
    click(&mut b, mm(3.0, 5.0), Modifiers::ALT);
    assert!(highlighted(&b).contains(&b.gnd[0]));
}
