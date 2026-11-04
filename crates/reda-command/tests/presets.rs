//! Presets (`command:preset`), availability and the catalogue.

mod support;

use reda_command::{Command, CommandError, Context, ErrorCode, Kind, Meta, Preset, Registry};
use serde_json::json;
use support::{Harness, RenameParams, ZoomLevel, pointer};

#[test]
fn a_preset_runs_its_command_with_its_fixed_params() {
    let mut h = Harness::with_project();
    let out = h.run("test.project.title:blinky", json!(null)).unwrap();
    assert_eq!(out["result"], json!({"old": "at 1767225600000"}));
    assert_eq!(h.name(), "Blinky");
}

#[test]
fn a_preset_refuses_params_of_its_own() {
    let mut h = Harness::with_project();
    let err = h
        .run("test.project.title:blinky", json!({"name": "Other"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidParams);
    assert_eq!(pointer(&err), "");
    assert_eq!(h.name(), "at 1767225600000");
}

#[test]
fn an_unknown_preset_is_not_found() {
    let mut h = Harness::with_project();
    let err = h.run("test.project.title:nope", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(err.message.contains("nope"), "{}", err.message);
}

#[test]
fn a_view_preset_is_unavailable_headless_and_runs_in_the_app() {
    let mut h = Harness::with_project();
    let err = h.run("test.view.zoom:double", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert_eq!(err.message, "requires the REDA app");
    h.headless = false;
    h.session.view.set(ZoomLevel(100));
    assert_eq!(
        h.run("test.view.zoom:double", json!({})).unwrap(),
        json!(200)
    );
}

#[test]
fn availability_explains_why_a_command_cannot_run() {
    let mut h = Harness::new();
    let unavailable = |h: &Harness, id: &str, headless: bool| {
        h.registry
            .available(id, h.session.project.as_ref(), headless)
            .err()
            .map(|e| e.message)
    };
    assert_eq!(
        unavailable(&h, "test.project.rename", true).as_deref(),
        Some("no Project is open")
    );
    assert_eq!(
        unavailable(&h, "test.view.zoom:actual", true).as_deref(),
        Some("requires the REDA app")
    );
    assert_eq!(unavailable(&h, "test.view.zoom", false), None);
    h.run("test.project.create", json!({})).unwrap();
    assert_eq!(unavailable(&h, "test.project.rename", true), None);
    assert_eq!(unavailable(&h, "batch", true), None);
}

#[test]
fn the_catalogue_lists_every_command_with_schemas_and_presets() {
    let h = Harness::new();
    let catalogue = h.registry.catalogue();
    let commands = catalogue["commands"].as_array().unwrap();
    let ids: Vec<&str> = commands.iter().map(|c| c["id"].as_str().unwrap()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    assert!(ids.contains(&"batch"));

    let zoom = h.registry.describe("test.view.zoom").unwrap();
    assert_eq!(zoom["title"], "Zoom");
    assert_eq!(zoom["kind"], "view");
    assert_eq!(zoom["icon"], "zoom");
    assert_eq!(zoom["category"], "view");
    assert_eq!(
        zoom["params"]["properties"]["level"]["description"],
        "Percent."
    );
    assert_eq!(zoom["result"]["type"], "integer");
    assert_eq!(
        zoom["presets"],
        json!([
            {"id": "test.view.zoom:actual", "title": "Actual Size", "icon": "zoom_actual", "params": {"level": 100}},
            {"id": "test.view.zoom:double", "title": "Zoom 200%", "icon": "zoom_in", "params": {"level": 200}},
        ])
    );
    let batch = h.registry.describe("batch").unwrap();
    assert_eq!(batch["kind"], "edit");
    assert!(batch["params"]["properties"]["commands"].is_object());
}

struct BadPreset;

impl Command for BadPreset {
    const META: Meta = Meta {
        id: "test.bad_preset",
        title: "Bad",
        description: "Has a Preset whose params do not fit.",
        category: "test",
        icon: "bug",
        kind: Kind::Edit,
    };
    type Params = RenameParams;
    type Output = ();

    fn presets(&self) -> Vec<Preset> {
        vec![Preset::new("wrong", "Wrong", "bug", json!({"name": 7}))]
    }

    fn run(&self, _: &mut Context<'_>, _: RenameParams) -> Result<(), CommandError> {
        Ok(())
    }
}

#[test]
#[should_panic(expected = "test.bad_preset:wrong")]
fn a_preset_whose_params_do_not_fit_panics_at_registration() {
    Registry::new().add(BadPreset);
}
