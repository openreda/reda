//! PROTOTYPE (throwaway, #398): renders the Board Template picker variants to PNGs.
//! `REDA_PROTO_OUT=<dir> cargo test -p reda-ui --test prototype_board_templates -- --nocapture`

use std::path::PathBuf;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::prototype_board_templates as proto;
use reda_ui::shell::Shell;

type Scene = (
    &'static str,
    usize,
    bool,
    &'static str,
    &'static [(&'static str, &'static str)],
    u8,
    &'static [&'static str],
);

const SCENES: &[Scene] = &[
    (
        "a-pi-full",
        0,
        false,
        "Add-on board for Raspberry Pi",
        &[],
        0,
        &[],
    ),
    (
        "a-pcie-x4",
        0,
        false,
        "PCI Express add-in card",
        &[("lanes", "4")],
        0,
        &[],
    ),
    (
        "a-pcb-feather",
        0,
        true,
        "Feather or FeatherWing",
        &[],
        0,
        &["h2"],
    ),
    ("b-gallery", 1, false, "Shield for Arduino Uno", &[], 0, &[]),
    (
        "b-setup-m2",
        1,
        false,
        "M.2 card",
        &[("length", "42"), ("key", "bm")],
        1,
        &[],
    ),
    (
        "c-pi-untick",
        2,
        false,
        "Add-on board for Raspberry Pi",
        &[("size", "micro")],
        0,
        &["h1"],
    ),
    (
        "c-pcb-pcie-error",
        2,
        true,
        "PCI Express add-in card",
        &[("lanes", "16"), ("bracket", "low"), ("card_length", "120")],
        0,
        &[],
    ),
    ("c-mega", 2, false, "Shield for Arduino Mega", &[], 0, &[]),
];

#[test]
fn render_variants() {
    let out = std::env::var_os("REDA_PROTO_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("proto-board"));
    std::fs::create_dir_all(&out).unwrap();
    for (name, variant, pcb, template, values, page, untick) in SCENES {
        let home = TempDir::new("proto-home");
        let engine = Engine::builder()
            .user_dirs(Some(UserDirs::under(home.path())))
            .starter_library(None)
            .headless(false)
            .build();
        let mut h = Harness::builder()
            .with_size([1440.0, 940.0])
            .wgpu()
            .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
        h.run();
        h.get_all_by_label("New Project…").last().unwrap().click();
        h.run();
        proto::preset(&h.ctx, *variant, *pcb, template, values, *page);
        for item in *untick {
            proto::untick(&h.ctx, item);
        }
        h.run();
        let image = h.render().unwrap();
        image.save(out.join(format!("{name}.png"))).unwrap();
    }
}
