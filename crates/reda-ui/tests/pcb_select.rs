//! The PCB selection Commands: `pcb.selection.on_layer`, `pcb.selection.net`,
//! `pcb.selection.physical_connection`, `pcb.selection.footprint_connections` and
//! `pcb.selection.footprint_nets`. Each replaces the selection.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{ErrorCode, SeededIds, UserDirs};
use reda_ui::engine::model::{PadRef, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

struct Board {
    shell: Shell,
    gnd: Uuid,
    sig: Uuid,
    /// U1 with Pads on GND (0, 0) and SIG (5, 0); U2 with Pads on GND (20, 0) and SIG
    /// (25, 0); R1 with one Pad on SIG at (25, 20).
    u1: (Uuid, [Uuid; 2]),
    u2: (Uuid, [Uuid; 2]),
    r1: (Uuid, Uuid),
    /// GND: top Track U1 → Via at (10, 0), bottom Track Via → (15, 0), top Track
    /// (15, 0) → U2. The Via joins the bottom Track at (10, 0) only.
    gnd_top: [Uuid; 2],
    gnd_via: Uuid,
    gnd_bottom: Uuid,
    /// SIG: U2's SIG Pad → (25, 10) → R1, two Tracks; and a stub off R1.
    sig_route: [Uuid; 2],
    sig_stub: Uuid,
    /// A silkscreen line.
    silk: Uuid,
    _home: TempDir,
    _work: TempDir,
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn uuid(value: &Value) -> Uuid {
    value.as_str().unwrap().parse().unwrap()
}

fn footprint(shell: &mut Shell, x: f64, pads: &[(f64, f64, Uuid)]) -> (Uuid, Vec<Uuid>) {
    let pads: Vec<Value> = pads
        .iter()
        .map(|(px, py, net)| {
            json!({"at": {"x": format!("{px}mm"), "y": format!("{py}mm")},
                   "width": "1mm", "height": "1mm", "net": net})
        })
        .collect();
    let out = run(
        shell,
        "test.footprint.add",
        json!({"at": {"x": format!("{x}mm"), "y": 0}, "pads": pads}),
    )["result"]
        .clone();
    let pads = out["pads"].as_array().unwrap().iter().map(uuid).collect();
    (uuid(&out["footprint"]), pads)
}

fn track(shell: &mut Shell, layer: &str, net: Uuid, from: (f64, f64), to: (f64, f64)) -> Uuid {
    uuid(
        &run(
            shell,
            "pcb.track.add",
            json!({"layer": layer, "net": net, "points": [
                {"x": format!("{}mm", from.0), "y": format!("{}mm", from.1)},
                {"x": format!("{}mm", to.0), "y": format!("{}mm", to.1)}]}),
        )["result"]["tracks"][0],
    )
}

fn board() -> Board {
    let home = TempDir::new("pcb-select-home");
    let work = TempDir::new("pcb-select-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let gnd = uuid(&run(&mut shell, "test.net.add", json!({"name": "GND"}))["result"]["uuid"]);
    let sig = uuid(&run(&mut shell, "test.net.add", json!({"name": "SIG"}))["result"]["uuid"]);
    let (u1, p1) = footprint(&mut shell, 0.0, &[(0.0, 0.0, gnd), (5.0, 0.0, sig)]);
    let (u2, p2) = footprint(&mut shell, 20.0, &[(0.0, 0.0, gnd), (5.0, 0.0, sig)]);
    let (r1, pr) = footprint(&mut shell, 25.0, &[(0.0, 20.0, sig)]);
    let a = track(&mut shell, "copper_top", gnd, (0.0, -0.0), (0.0, -3.0));
    let b = track(&mut shell, "copper_top", gnd, (0.0, -3.0), (10.0, -3.0));
    let gnd_via = uuid(
        &run(
            &mut shell,
            "pcb.via.add",
            json!({"at": {"x": "10mm", "y": "-3mm"}, "net": gnd}),
        )["result"]["uuid"],
    );
    let gnd_bottom = track(&mut shell, "copper_bottom", gnd, (10.0, -3.0), (20.0, -3.0));
    // A second Via at (20, -3) would join U2; leave the bottom Track ending there.
    let s1 = track(&mut shell, "copper_top", sig, (25.0, 0.0), (25.0, 10.0));
    let s2 = track(&mut shell, "copper_top", sig, (25.0, 10.0), (25.0, 20.0));
    let stub = track(&mut shell, "copper_top", sig, (25.0, 20.0), (30.0, 20.0));
    let silk = uuid(
        &run(
            &mut shell,
            "pcb.graphic.add_line",
            json!({"layer": "silkscreen_top", "width": "0.2mm",
                   "from": {"x": 0, "y": "30mm"}, "to": {"x": "10mm", "y": "30mm"}}),
        )["result"]["uuid"],
    );
    Board {
        shell,
        gnd,
        sig,
        u1: (u1, [p1[0], p1[1]]),
        u2: (u2, [p2[0], p2[1]]),
        r1: (r1, pr[0]),
        gnd_top: [a, b],
        gnd_via,
        gnd_bottom,
        sig_route: [s1, s2],
        sig_stub: stub,
        silk,
        _home: home,
        _work: work,
    }
}

fn pad(footprint: Uuid, pad: Uuid) -> PadRef {
    PadRef { footprint, pad }
}

fn sorted(mut v: Vec<Uuid>) -> Vec<Uuid> {
    v.sort();
    v
}

fn sorted_pads(mut v: Vec<PadRef>) -> Vec<PadRef> {
    v.sort_by_key(|p| (p.footprint, p.pad));
    v
}

fn selected(b: &Board) -> (Vec<Uuid>, Vec<PadRef>) {
    let s = b.shell.selection();
    (sorted(s.objects.clone()), sorted_pads(s.pads.clone()))
}

#[test]
fn on_layer_selects_everything_on_a_layer() {
    let mut b = board();
    run(&mut b.shell, "selection.set", json!({"objects": [b.silk]}));
    let out = run(
        &mut b.shell,
        "pcb.selection.on_layer",
        json!({"layer": "copper_bottom"}),
    );
    let (objects, pads) = selected(&b);
    assert_eq!(
        objects,
        sorted(vec![b.gnd_via, b.gnd_bottom]),
        "replaces the selection"
    );
    assert!(pads.is_empty(), "the Pads are SMD on top");
    assert_eq!(out["count"], json!(2));

    run(
        &mut b.shell,
        "pcb.selection.on_layer",
        json!({"layer": "copper_top"}),
    );
    let (objects, pads) = selected(&b);
    let mut expected = vec![b.gnd_via, b.sig_stub];
    expected.extend(b.gnd_top);
    expected.extend(b.sig_route);
    assert_eq!(objects, sorted(expected));
    assert_eq!(
        pads,
        sorted_pads(vec![
            pad(b.u1.0, b.u1.1[0]),
            pad(b.u1.0, b.u1.1[1]),
            pad(b.u2.0, b.u2.1[0]),
            pad(b.u2.0, b.u2.1[1]),
            pad(b.r1.0, b.r1.1),
        ])
    );

    run(
        &mut b.shell,
        "pcb.selection.on_layer",
        json!({"layer": "silkscreen_top"}),
    );
    assert_eq!(selected(&b).0, vec![b.silk]);

    let err = b
        .shell
        .run("pcb.selection.on_layer", json!({"layer": "nowhere"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn net_selects_all_copper_and_pads_of_a_net() {
    let mut b = board();
    run(&mut b.shell, "pcb.selection.net", json!({"net": b.sig}));
    let (objects, pads) = selected(&b);
    let mut expected = vec![b.sig_stub];
    expected.extend(b.sig_route);
    assert_eq!(objects, sorted(expected));
    assert_eq!(
        pads,
        sorted_pads(vec![
            pad(b.u1.0, b.u1.1[1]),
            pad(b.u2.0, b.u2.1[1]),
            pad(b.r1.0, b.r1.1),
        ])
    );
    let err = b
        .shell
        .run("pcb.selection.net", json!({"net": Uuid::from_u128(1)}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn physical_connection_follows_tracks_and_vias_up_to_the_next_pad() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.selection.physical_connection",
        json!({"seed": b.gnd_top[1]}),
    );
    let mut expected = vec![b.gnd_via, b.gnd_bottom];
    expected.extend(b.gnd_top);
    assert_eq!(
        selected(&b).0,
        sorted(expected),
        "through the Via, stopping at U1's Pad"
    );

    run(
        &mut b.shell,
        "pcb.selection.physical_connection",
        json!({"seed": b.gnd_top[1], "single_layer": true}),
    );
    assert_eq!(
        selected(&b).0,
        sorted(b.gnd_top.to_vec()),
        "top Tracks only"
    );

    run(
        &mut b.shell,
        "pcb.selection.physical_connection",
        json!({"seed": b.sig_route[0]}),
    );
    assert_eq!(
        selected(&b).0,
        sorted(b.sig_route.to_vec()),
        "R1's Pad stops it before the stub"
    );

    let err = b
        .shell
        .run("pcb.selection.physical_connection", json!({"seed": b.silk}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn footprint_connections_and_nets_select_from_the_footprints_pads() {
    let mut b = board();
    run(
        &mut b.shell,
        "pcb.selection.footprint_connections",
        json!({"footprints": [b.r1.0]}),
    );
    let mut expected = vec![b.sig_stub];
    expected.extend(b.sig_route);
    assert_eq!(selected(&b), (sorted(expected), vec![]));

    run(
        &mut b.shell,
        "pcb.selection.footprint_connections",
        json!({"footprints": [b.u1.0]}),
    );
    let mut expected = vec![b.gnd_via, b.gnd_bottom];
    expected.extend(b.gnd_top);
    assert_eq!(selected(&b).0, sorted(expected));

    run(
        &mut b.shell,
        "pcb.selection.footprint_nets",
        json!({"footprints": [b.u1.0]}),
    );
    let (objects, pads) = selected(&b);
    let mut expected = vec![b.gnd_via, b.gnd_bottom, b.sig_stub];
    expected.extend(b.gnd_top);
    expected.extend(b.sig_route);
    assert_eq!(objects, sorted(expected), "both of U1's Nets");
    assert_eq!(pads.len(), 5, "every Pad on GND or SIG");
    let _ = b.gnd;

    let err = b
        .shell
        .run(
            "pcb.selection.footprint_nets",
            json!({"footprints": [Uuid::from_u128(2)]}),
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
}
