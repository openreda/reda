//! Drives a registry of toy Commands the way the engine, MCP and the CLI do.

use reda_command::{
    Clock, Command, CommandError, Context, Env, ErrorCode, FixedClock, Kind, Meta, NoParams,
    Registry, SeededIds, Session,
};
use reda_model::{Object, Project, ProjectProps, Scope, Uuid};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

struct Rename;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenameParams {
    name: String,
}

#[derive(Serialize)]
struct RenameResult {
    old: String,
}

impl Command for Rename {
    const META: Meta = Meta {
        id: "test.project.rename",
        title: "Rename Project",
        description: "Renames the Project.",
        category: "project",
        icon: "rename",
        kind: Kind::Edit,
    };
    type Params = RenameParams;
    type Output = RenameResult;

    fn run(&self, cx: &mut Context<'_>, p: RenameParams) -> Result<RenameResult, CommandError> {
        if p.name.is_empty() {
            return Err(CommandError::rejected("a Project needs a name"));
        }
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        let old = std::mem::replace(&mut props.name, p.name.clone());
        tx.replace(Object::Project(props))?;
        if p.name == "fail-after-edit" {
            return Err(CommandError::rejected("refused after editing"));
        }
        Ok(RenameResult { old })
    }
}

struct Name;

impl Command for Name {
    const META: Meta = Meta {
        id: "test.project.name",
        title: "Project name",
        description: "Returns the Project name.",
        category: "project",
        icon: "info",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = String;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<String, CommandError> {
        Ok(cx.project()?.props().name.clone())
    }
}

/// A query that wrongly asks for a transaction.
struct SneakyQuery;

impl Command for SneakyQuery {
    const META: Meta = Meta {
        id: "test.sneaky",
        title: "Sneaky",
        description: "A query that tries to edit.",
        category: "test",
        icon: "bug",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<(), CommandError> {
        cx.tx()?;
        Ok(())
    }
}

/// A file Command that creates a fresh Project with the injected ids.
struct Create;

impl Command for Create {
    const META: Meta = Meta {
        id: "test.project.create",
        title: "Create",
        description: "Replaces the session's Project with a new one.",
        category: "project",
        icon: "new",
        kind: Kind::File,
    };
    type Params = NoParams;
    type Output = Uuid;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Uuid, CommandError> {
        let uuid = cx.new_id();
        cx.session()?.project = Some(Project::new(ProjectProps {
            uuid,
            name: format!("at {}", cx.now().0),
            documents: vec![],
        }));
        Ok(uuid)
    }
}

struct Zoom;

impl Command for Zoom {
    const META: Meta = Meta {
        id: "test.view.zoom",
        title: "Zoom",
        description: "Zooms.",
        category: "view",
        icon: "zoom",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = ();

    fn run(&self, _: &mut Context<'_>, _: NoParams) -> Result<(), CommandError> {
        Ok(())
    }
}

fn register_all(registry: &mut Registry) {
    registry.add(Rename);
    registry.add(Name);
    registry.add(SneakyQuery);
    registry.add(Create);
    registry.add(Zoom);
}

struct Harness {
    registry: Registry,
    session: Session,
    ids: SeededIds,
    clock: FixedClock,
}

impl Harness {
    fn new() -> Harness {
        let mut registry = Registry::new();
        register_all(&mut registry);
        Harness {
            registry,
            session: Session::default(),
            ids: SeededIds::new(0),
            clock: FixedClock::EPOCH,
        }
    }

    fn with_project() -> Harness {
        let mut h = Harness::new();
        h.run("test.project.create", json!({})).unwrap();
        h
    }

    fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let mut env = Env {
            ids: &mut self.ids,
            clock: &self.clock,
            headless: true,
        };
        self.registry.run(&mut self.session, &mut env, id, params)
    }

    fn name(&self) -> String {
        self.session.project.as_ref().unwrap().props().name.clone()
    }
}

#[test]
fn edit_returns_the_change_envelope_and_dirties_its_scope() {
    let mut h = Harness::with_project();
    let project = h.session.project.as_ref().unwrap().uuid();
    let out = h
        .run("test.project.rename", json!({"name": "Blinky"}))
        .unwrap();
    assert_eq!(
        out,
        json!({
            "changes": {"project": {"created": [], "modified": [project.to_string()], "deleted": []}},
            "result": {"old": "at 1767225600000"}
        })
    );
    assert_eq!(h.name(), "Blinky");
    assert!(h.session.dirty.contains(&Scope::Project));
}

#[test]
fn a_failed_edit_rolls_back_fully() {
    let mut h = Harness::with_project();
    let err = h
        .run("test.project.rename", json!({"name": "fail-after-edit"}))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Rejected);
    assert_eq!(h.name(), "at 1767225600000");
    assert!(h.session.dirty.is_empty());
}

#[test]
fn query_returns_its_result_bare() {
    let mut h = Harness::with_project();
    assert_eq!(
        h.run("test.project.name", json!({})).unwrap(),
        json!("at 1767225600000")
    );
}

#[test]
fn only_edit_commands_may_open_a_transaction() {
    let mut h = Harness::with_project();
    let err = h.run("test.sneaky", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[test]
fn bad_params_are_invalid_params() {
    let mut h = Harness::with_project();
    for params in [
        json!({"name": 3}),
        json!({}),
        json!({"name": "x", "extra": 1}),
    ] {
        let err = h.run("test.project.rename", params.clone()).unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams, "{params}");
    }
}

#[test]
fn unknown_command_is_not_found() {
    let mut h = Harness::new();
    let err = h.run("test.nope", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    assert!(err.message.contains("test.nope"), "{}", err.message);
}

#[test]
fn view_commands_are_unavailable_headless() {
    let mut h = Harness::with_project();
    let err = h.run("test.view.zoom", json!({})).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unavailable);
    assert_eq!(err.message, "requires the REDA app");
}

#[test]
fn edits_and_queries_need_an_open_project() {
    let mut h = Harness::new();
    for id in ["test.project.rename", "test.project.name"] {
        let err = h.run(id, json!({"name": "x"})).unwrap_err();
        assert_eq!(err.code, ErrorCode::Unavailable, "{id}");
    }
}

#[test]
fn file_commands_use_the_injected_ids_and_clock() {
    let mut h = Harness::new();
    let out = h.run("test.project.create", json!({})).unwrap();
    assert_eq!(out, json!("00000000-0000-4000-8000-000000000001"));
    assert_eq!(h.name(), format!("at {}", FixedClock::EPOCH.now().0));
}

#[test]
fn errors_serialise_as_code_and_message() {
    let err = CommandError::not_found("no such thing");
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({"code": "not_found", "message": "no such thing"})
    );
    let err = CommandError::invalid_params("/name", "expected a string");
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({"code": "invalid_params", "message": "expected a string", "details": {"pointer": "/name"}})
    );
}

#[test]
fn the_registry_lists_metadata_sorted_by_id() {
    let h = Harness::new();
    let ids: Vec<&str> = h.registry.commands().map(|m| m.id).collect();
    assert_eq!(
        ids,
        [
            "test.project.create",
            "test.project.name",
            "test.project.rename",
            "test.sneaky",
            "test.view.zoom"
        ]
    );
    assert_eq!(h.registry.get("test.view.zoom").unwrap().kind, Kind::View);
}

#[test]
#[should_panic(expected = "test.project.name")]
fn registering_an_id_twice_panics() {
    let mut registry = Registry::new();
    registry.add(Name);
    registry.add(Name);
}
