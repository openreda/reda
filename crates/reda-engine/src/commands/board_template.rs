//! `library.board_template.list` and `library.board_template.preview`: the Board
//! Templates of every registered Library, and one evaluated at a set of values.
//!
//! Both are read-only and work with no Project open, so New Project can call them. They
//! are file Commands, as `library.search` is, because they scan the Library folders on
//! disk.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_library::board_template::{
    BoardTemplateChoice, Frame, ResolvedValue, TemplateError, constant_length, evaluate,
};
use reda_library::{Library, LibraryIndex, Origin, ScanProblem};
use reda_model::geom::Nm;
use reda_model::{BoardTemplate, DisplayUnit, Side, TemplateParameter, Uuid, When};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::library::scan_registered;

pub(crate) mod create;
pub use create::{SeededObject, SeededTemplate};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(BoardTemplateList);
    registry.add(BoardTemplatePreview);
}

/// The template with this element UUID in the scanned Libraries, and its Library.
pub fn find_board_template(
    index: &LibraryIndex,
    element: Uuid,
) -> Result<(&Library, &BoardTemplate), CommandError> {
    let template = index.board_template(element).ok_or_else(|| {
        CommandError::not_found(format!(
            "no registered Library holds a usable Board Template {element}; \
             `library.board_template.list` lists them and their problems"
        ))
    })?;
    let library = index
        .library_of(element)
        .expect("a found template has a Library");
    Ok((library, template))
}

/// The template `choice` names, evaluated: the one path preview and creation share.
/// Bad `values` or `exclude` are `invalid_params`; an expression that fails at these
/// values is `rejected`. The frame's own problems are left for the caller to judge.
pub fn evaluate_choice(
    index: &LibraryIndex,
    choice: &BoardTemplateChoice,
) -> Result<Frame, CommandError> {
    let (_, template) = find_board_template(index, choice.element)?;
    evaluate(template, choice).map_err(template_error)
}

/// A [`TemplateError`] as the Command error it is.
pub fn template_error(e: TemplateError) -> CommandError {
    if e.pointer.is_empty() {
        CommandError::rejected(e.message)
    } else {
        CommandError::invalid_params(&e.pointer, e.message)
    }
}

pub struct BoardTemplateList;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ListParams {
    /// Only the templates of the Library with this UUID.
    pub library: Option<Uuid>,
}

/// One Board Template that passed its load checks.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListedTemplate {
    /// Its Library element UUID: what `board_template.element` names.
    pub element: Uuid,
    pub name: String,
    pub description: String,
    /// Groups the picker; `None` lists it under "Other".
    pub category: Option<String>,
    /// The Library's UUID.
    pub library: Uuid,
    /// The Library's origin. New Project offers only `user` and `starter` templates.
    pub origin: Origin,
    /// At the default values.
    pub suggested_layers: u8,
    pub parameters: Vec<ListedParameter>,
    pub objects: Vec<ListedObject>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    Choice,
    Length,
}

/// One parameter, as the dialog shows it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListedParameter {
    pub name: String,
    pub kind: ParameterKind,
    pub label: String,
    /// A choice's options, in order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<ListedOption>>,
    /// A choice's default option key, or a length's default in nm.
    pub default: ResolvedValue,
    /// A length's least value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Nm>,
    /// A length's greatest value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Nm>,
    /// The unit a length is shown in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<DisplayUnit>,
    /// The dialog hides the parameter unless this holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<When>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListedOption {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Outline,
    Cutout,
    Keepout,
    Connector,
    Hole,
    DrawingGroup,
    Drawing,
}

/// One template object row. `exclude` may name connectors, holes and drawing groups.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ListedObject {
    /// The template-local UUID.
    pub uuid: Uuid,
    pub kind: ObjectKind,
    /// A connector's or Mechanical Part hole's Part, by Library element UUID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<Uuid>,
    /// That Part's name, for grouping the Includes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
    /// A Keepout's connector or hole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protects: Option<Uuid>,
    /// A drawing's group.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<Uuid>,
    /// A drawing group's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether this row has a `when` gate, so it may be absent at some values.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub gated: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ListResult {
    /// The usable templates: Library by Library in resolution order (Project, user,
    /// Starter), each Library's by name.
    pub templates: Vec<ListedTemplate>,
    /// Folders and files the scan could not use, including templates that failed their
    /// load checks.
    pub problems: Vec<ScanProblem>,
}

fn listed(library: &Library, t: &BoardTemplate) -> ListedTemplate {
    let defaults = evaluate(
        t,
        &BoardTemplateChoice {
            element: t.uuid,
            ..BoardTemplateChoice::default()
        },
    )
    .ok();
    let value = |name: &str| defaults.as_ref().and_then(|f| f.values.get(name).cloned());
    let parameters = t
        .parameters
        .iter()
        .map(|p| match p {
            TemplateParameter::Choice(c) => ListedParameter {
                name: c.name.clone(),
                kind: ParameterKind::Choice,
                label: c.label.clone(),
                options: Some(
                    c.options
                        .iter()
                        .map(|o| ListedOption {
                            key: o.key.clone(),
                            label: o.label.clone(),
                        })
                        .collect(),
                ),
                default: ResolvedValue::Choice(c.default_key().unwrap_or_default().to_owned()),
                min: None,
                max: None,
                unit: None,
                when: c.when.clone(),
            },
            TemplateParameter::Length(l) => ListedParameter {
                name: l.name.clone(),
                kind: ParameterKind::Length,
                label: l.label.clone(),
                options: None,
                default: value(&l.name)
                    .unwrap_or_else(|| ResolvedValue::Length(length_of(&l.default))),
                min: Some(length_of(&l.min)),
                max: Some(length_of(&l.max)),
                unit: Some(l.unit),
                when: l.when.clone(),
            },
        })
        .collect();
    let part_of = |entry: Uuid| t.parts.iter().find(|p| p.uuid == entry).map(|p| &p.part);
    let mut objects = vec![];
    let row = |uuid, kind, gated: bool| ListedObject {
        uuid,
        kind,
        part: None,
        part_name: None,
        side: None,
        protects: None,
        group: None,
        name: None,
        gated,
    };
    for r in &t.outline {
        objects.push(row(r.uuid, ObjectKind::Outline, r.when.is_some()));
    }
    for r in &t.cutouts {
        objects.push(row(r.uuid, ObjectKind::Cutout, r.when.is_some()));
    }
    for r in &t.keepouts {
        objects.push(ListedObject {
            protects: r.protects,
            ..row(r.uuid, ObjectKind::Keepout, r.when.is_some())
        });
    }
    for r in &t.connectors {
        let part = part_of(r.part);
        objects.push(ListedObject {
            part: part.map(|p| p.uuid),
            part_name: part.map(|p| p.name.clone()),
            side: Some(r.side),
            ..row(r.uuid, ObjectKind::Connector, r.when.is_some())
        });
    }
    for r in &t.holes {
        let part = r.part.and_then(part_of);
        objects.push(ListedObject {
            part: part.map(|p| p.uuid),
            part_name: part.map(|p| p.name.clone()),
            side: r.side,
            ..row(r.uuid, ObjectKind::Hole, r.when.is_some())
        });
    }
    for g in &t.drawing_groups {
        objects.push(ListedObject {
            name: Some(g.name.clone()),
            ..row(g.uuid, ObjectKind::DrawingGroup, g.when.is_some())
        });
    }
    for d in &t.drawings {
        objects.push(ListedObject {
            group: Some(d.group),
            ..row(d.uuid, ObjectKind::Drawing, d.when.is_some())
        });
    }
    ListedTemplate {
        element: t.uuid,
        name: t.name.clone(),
        description: t.description.clone(),
        category: t.category.clone(),
        library: library.info.uuid,
        origin: library.origin,
        suggested_layers: defaults.map_or(t.suggested_layers, |f| f.suggested_layers),
        parameters,
        objects,
    }
}

/// A length bound, which the load checks made a constant.
fn length_of(n: &reda_model::TemplateNumber) -> Nm {
    constant_length(n).unwrap_or(Nm(0))
}

impl Command for BoardTemplateList {
    const META: Meta = Meta {
        id: "library.board_template.list",
        title: "List Board Templates",
        description: "Scans the registered Libraries (the open Project's, if any, the user's \
                      and the starter Library) and returns every usable Board Template with \
                      its Library's `origin`, parameters and object rows, plus the scan's \
                      problems, including templates that failed their load checks. Works \
                      with no Project open.",
        category: "library",
        icon: "board_outline_rect",
        kind: Kind::File,
    };
    type Params = ListParams;
    type Output = ListResult;

    fn run(&self, cx: &mut Context<'_>, p: ListParams) -> Result<ListResult, CommandError> {
        let index = scan_registered(cx.session()?);
        let templates = index
            .board_templates()
            .filter(|(library, _)| p.library.is_none_or(|l| l == library.info.uuid))
            .map(|(library, t)| listed(library, t))
            .collect();
        Ok(ListResult {
            templates,
            problems: index.problems().to_vec(),
        })
    }
}

pub struct BoardTemplatePreview;

impl Command for BoardTemplatePreview {
    const META: Meta = Meta {
        id: "library.board_template.preview",
        title: "Preview Board Template",
        description: "Evaluates the Board Template `element` at `values`, leaving out \
                      `exclude`, and returns the frame creation would write: outline and \
                      cutout vertices with their Plated Edge marks, Keepouts, holes and \
                      connectors as placed with their pad outlines, drawings by group, the \
                      resolved values and suggested layer count, and the Board Outline, \
                      Board Cutout and Keepout problems with their locations. Works with no \
                      Project open.",
        category: "library",
        icon: "board_outline_rect",
        kind: Kind::File,
    };
    type Params = BoardTemplateChoice;
    type Output = Frame;

    fn run(&self, cx: &mut Context<'_>, p: BoardTemplateChoice) -> Result<Frame, CommandError> {
        let index = scan_registered(cx.session()?);
        evaluate_choice(&index, &p)
    }
}
