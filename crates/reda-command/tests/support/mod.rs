//! Toy Commands and a harness that drives a registry the way the engine, MCP and the CLI do.

#![allow(dead_code)]

use reda_command::{
    Angle, Command, CommandError, Context, Env, FixedClock, Kind, Length, Meta, NoParams, Preset,
    Registry, SeededIds, Session,
};
use reda_model::geom::{Millideg, Nm};
use reda_model::{DocumentEntry, DocumentKind, Object, Project, ProjectProps, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub struct Rename;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameParams {
    name: String,
}

#[derive(Serialize, JsonSchema)]
pub struct RenameResult {
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

/// An edit that adds a Schematic Sheet, with an optional caller-supplied UUID.
pub struct AddSheet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddSheetParams {
    #[serde(default)]
    uuid: Option<Uuid>,
    title: String,
}

impl Command for AddSheet {
    const META: Meta = Meta {
        id: "test.sheet.add",
        title: "Add Sheet",
        description: "Adds a Schematic Sheet.",
        category: "project",
        icon: "sheet_add",
        kind: Kind::Edit,
    };
    type Params = AddSheetParams;
    type Output = Uuid;

    fn run(&self, cx: &mut Context<'_>, p: AddSheetParams) -> Result<Uuid, CommandError> {
        let uuid = p.uuid.unwrap_or_else(|| cx.new_id());
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        props.documents.push(uuid);
        tx.insert(Object::Document(DocumentEntry {
            uuid,
            kind: DocumentKind::Schematic,
            name: p.title.to_lowercase(),
            title: p.title,
        }))?;
        tx.replace(Object::Project(props))?;
        Ok(uuid)
    }
}

/// An edit that retitles an existing Document, found by UUID.
pub struct RetitleSheet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetitleSheetParams {
    document: Uuid,
    title: String,
}

impl Command for RetitleSheet {
    const META: Meta = Meta {
        id: "test.sheet.retitle",
        title: "Retitle Sheet",
        description: "Changes a Document's title.",
        category: "project",
        icon: "rename",
        kind: Kind::Edit,
    };
    type Params = RetitleSheetParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: RetitleSheetParams) -> Result<(), CommandError> {
        let tx = cx.tx()?;
        let mut entry = tx
            .project()
            .document(p.document)
            .cloned()
            .ok_or_else(|| CommandError::not_found(format!("no Document {}", p.document)))?;
        entry.title = p.title;
        tx.replace(Object::Document(entry))?;
        Ok(())
    }
}

pub struct Name;

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

/// A query with length and angle params, echoing them back as integers.
pub struct Measure;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasureParams {
    width: Length,
    #[serde(default)]
    angle: Option<Angle>,
    #[serde(default)]
    points: Vec<MeasurePoint>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasurePoint {
    x: Length,
    y: Length,
}

#[derive(Serialize, JsonSchema)]
pub struct MeasureResult {
    width: Nm,
    angle: Option<Millideg>,
    sums: Vec<Nm>,
}

impl Command for Measure {
    const META: Meta = Meta {
        id: "test.measure",
        title: "Measure",
        description: "Echoes lengths and angles.",
        category: "test",
        icon: "ruler",
        kind: Kind::Query,
    };
    type Params = MeasureParams;
    type Output = MeasureResult;

    fn run(&self, _: &mut Context<'_>, p: MeasureParams) -> Result<MeasureResult, CommandError> {
        Ok(MeasureResult {
            width: p.width.nm(),
            angle: p.angle.map(Angle::millideg),
            sums: p.points.iter().map(|pt| pt.x.nm() + pt.y.nm()).collect(),
        })
    }
}

/// A query that wrongly asks for a transaction.
pub struct SneakyQuery;

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
pub struct Create;

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
            libraries: vec![],
            properties: Default::default(),
        }));
        Ok(uuid)
    }
}

/// A view Command with Presets.
pub struct Zoom;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ZoomParams {
    /// Percent.
    level: u32,
}

impl Command for Zoom {
    const META: Meta = Meta {
        id: "test.view.zoom",
        title: "Zoom",
        description: "Zooms.",
        category: "view",
        icon: "zoom",
        kind: Kind::View,
    };
    type Params = ZoomParams;
    type Output = u32;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new(
                "actual",
                "Actual Size",
                "zoom_actual",
                json!({"level": 100}),
            ),
            Preset::new("double", "Zoom 200%", "zoom_in", json!({"level": 200})),
        ]
    }

    fn run(&self, _: &mut Context<'_>, p: ZoomParams) -> Result<u32, CommandError> {
        Ok(p.level)
    }
}

/// An edit with a Preset, so Presets can run headless.
pub struct Title;

impl Command for Title {
    const META: Meta = Meta {
        id: "test.project.title",
        title: "Set Title",
        description: "Renames the Project to a Preset's name.",
        category: "project",
        icon: "rename",
        kind: Kind::Edit,
    };
    type Params = RenameParams;
    type Output = RenameResult;

    fn presets(&self) -> Vec<Preset> {
        vec![Preset::new(
            "blinky",
            "Call it Blinky",
            "rename",
            json!({"name": "Blinky"}),
        )]
    }

    fn run(&self, cx: &mut Context<'_>, p: RenameParams) -> Result<RenameResult, CommandError> {
        Rename.run(cx, p)
    }
}

pub fn register_all(registry: &mut Registry) {
    registry.add(AddSheet);
    registry.add(Create);
    registry.add(Measure);
    registry.add(Name);
    registry.add(Rename);
    registry.add(RetitleSheet);
    registry.add(SneakyQuery);
    registry.add(Title);
    registry.add(Zoom);
}

pub struct Harness {
    pub registry: Registry,
    pub session: Session,
    pub ids: SeededIds,
    pub clock: FixedClock,
    pub headless: bool,
}

impl Harness {
    pub fn new() -> Harness {
        let mut registry = Registry::new();
        register_all(&mut registry);
        Harness {
            registry,
            session: Session::default(),
            ids: SeededIds::new(0),
            clock: FixedClock::EPOCH,
            headless: true,
        }
    }

    pub fn with_project() -> Harness {
        let mut h = Harness::new();
        h.run("test.project.create", json!({})).unwrap();
        h
    }

    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let mut env = Env {
            ids: &mut self.ids,
            clock: &self.clock,
            parts: &reda_command::NoLibraries,
            headless: self.headless,
        };
        self.registry.run(&mut self.session, &mut env, id, params)
    }

    pub fn project(&self) -> &Project {
        self.session.project.as_ref().unwrap()
    }

    pub fn name(&self) -> String {
        self.project().props().name.clone()
    }
}

/// The JSON pointer of an `invalid_params` error.
pub fn pointer(err: &CommandError) -> &str {
    err.details.as_ref().unwrap()["pointer"].as_str().unwrap()
}
