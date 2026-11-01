//! `project.overview`, `object.get` and `objects.query`: read-only views of the model for
//! agents and the CLI.

use std::collections::{BTreeMap, BTreeSet};

use reda_command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use reda_model::{DocumentKind, Object, ObjectType, Project, Scope, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ProjectOverview);
    registry.add(ObjectGet);
    registry.add(ObjectsQuery);
}

/// An object and the Document it lives in.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ObjectView {
    /// The Document holding the object, the UUID of an open Library element itself, or
    /// `null` for objects in the Project file.
    pub document: Option<Uuid>,
    pub object: Object,
}

impl ObjectView {
    fn of(object: Object) -> ObjectView {
        let document = match object.scope() {
            Scope::Project => None,
            Scope::Document(uuid) | Scope::Element(uuid) => Some(uuid),
        };
        ObjectView { document, object }
    }
}

pub struct ProjectOverview;

/// What `project.overview` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Overview {
    pub uuid: Uuid,
    pub name: String,
    /// In Project order.
    pub documents: Vec<DocumentOverview>,
}

/// One Document in the overview.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DocumentOverview {
    pub uuid: Uuid,
    pub kind: DocumentKind,
    /// The file stem.
    pub name: String,
    pub title: String,
    /// How many objects of each type the Document holds. Types with none are left out.
    pub objects: BTreeMap<ObjectType, usize>,
}

impl Command for ProjectOverview {
    const META: Meta = Meta {
        id: "project.overview",
        title: "Project Overview",
        description: "Summarises the open Project: its name and UUID, and each Document in \
                      Project order with its kind, title and how many objects of each type \
                      it holds.",
        category: "project",
        icon: "overview",
        kind: Kind::Query,
    };
    type Params = NoParams;
    type Output = Overview;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Overview, CommandError> {
        let project = cx.project()?;
        let mut counts: BTreeMap<Uuid, BTreeMap<ObjectType, usize>> = BTreeMap::new();
        for object in project.objects() {
            if let Scope::Document(document) = object.scope() {
                *counts
                    .entry(document)
                    .or_default()
                    .entry(object.object_type())
                    .or_default() += 1;
            }
        }
        let documents = project
            .documents()
            .map(|d| DocumentOverview {
                uuid: d.uuid,
                kind: d.kind,
                name: d.name.clone(),
                title: d.title.clone(),
                objects: counts.remove(&d.uuid).unwrap_or_default(),
            })
            .collect();
        Ok(Overview {
            uuid: project.uuid(),
            name: project.props().name.clone(),
            documents,
        })
    }
}

pub struct ObjectGet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectGetParams {
    /// The object's UUID.
    pub uuid: Uuid,
}

impl Command for ObjectGet {
    const META: Meta = Meta {
        id: "object.get",
        title: "Get Object",
        description: "Returns the object with this UUID, whatever its type, and the Document \
                      it lives in.",
        category: "query",
        icon: "inspect",
        kind: Kind::Query,
    };
    type Params = ObjectGetParams;
    type Output = ObjectView;

    fn run(&self, cx: &mut Context<'_>, p: ObjectGetParams) -> Result<ObjectView, CommandError> {
        let object = cx
            .project()?
            .object(p.uuid)
            .ok_or_else(|| CommandError::not_found(format!("no object has UUID {}", p.uuid)))?;
        Ok(ObjectView::of(object))
    }
}

pub struct ObjectsQuery;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectsQueryParams {
    /// Only objects in this Document. Document entries themselves live in the Project file.
    #[serde(default)]
    pub document: Option<Uuid>,
    /// Only objects of this type.
    #[serde(default, rename = "type")]
    pub object_type: Option<ObjectType>,
    /// Only objects on the schematic Nets with this name (a Net Label name or an
    /// auto-name such as `Net-(R1-1)`), derived from the drawing: their Wires, Net Labels,
    /// No-Connect Markers and the Units with a Pin on them.
    #[serde(default)]
    pub net: Option<String>,
}

/// What `objects.query` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct QueryResult {
    /// Sorted by UUID.
    pub objects: Vec<ObjectView>,
}

impl Command for ObjectsQuery {
    const META: Meta = Meta {
        id: "objects.query",
        title: "Query Objects",
        description: "Lists the objects that match every given filter, sorted by UUID: \
                      `document` keeps the objects in one Document, `type` the objects of \
                      one type, `net` the objects on the schematic Nets with that name. \
                      With no filter it lists every object in the Project.",
        category: "query",
        icon: "search",
        kind: Kind::Query,
    };
    type Params = ObjectsQueryParams;
    type Output = QueryResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ObjectsQueryParams,
    ) -> Result<QueryResult, CommandError> {
        let project = cx.project()?;
        if let Some(document) = p.document {
            check_document(project, document)?;
        }
        let on_net: Option<BTreeSet<Uuid>> = p.net.as_deref().map(|name| {
            reda_schematic::connectivity::project_connectivity(project)
                .iter()
                .flat_map(|sheet| sheet.objects_named(name))
                .collect()
        });
        let objects = project
            .objects()
            .into_iter()
            .filter(|o| on_net.as_ref().is_none_or(|set| set.contains(&o.uuid())))
            .filter(|o| p.object_type.is_none_or(|t| o.object_type() == t))
            .filter(|o| p.document.is_none_or(|d| o.scope() == Scope::Document(d)))
            .map(ObjectView::of)
            .collect();
        Ok(QueryResult { objects })
    }
}

fn check_document(project: &Project, document: Uuid) -> Result<(), CommandError> {
    match project.document(document) {
        Some(_) => Ok(()),
        None => Err(CommandError::not_found(format!(
            "no Document has UUID {document}"
        ))),
    }
}
