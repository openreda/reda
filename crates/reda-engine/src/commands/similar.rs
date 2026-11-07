//! `objects.find_similar`: Select Similar as a query. It lists the objects like a template
//! object, judged attribute by attribute, on the template's sheet, across the Project, on
//! the board or in the open Library element the template belongs to.
//!
//! An object's attributes are its fields as the object schema names them, nested ones by
//! dotted path (`fields.value`, `at.x`), and `type` for its kind. A Pad of a Footprint
//! instance has its Footprint copy's Pad fields plus `footprint` and `net`.

use std::collections::BTreeMap;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{LibraryElement, Object, PadRef, Project, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(FindSimilar);
}

/// How one attribute of a candidate must compare with the template's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    /// Whatever it is.
    Any,
    /// Equal to the template's.
    Same,
    /// Unlike the template's.
    Different,
    /// Its text matches this pattern, where `*` stands for any run of characters,
    /// ignoring case.
    Pattern(String),
}

impl<'de> Deserialize<'de> for Rule {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Rule, D::Error> {
        let text = String::deserialize(d)?;
        Ok(match text.as_str() {
            "any" => Rule::Any,
            "same" => Rule::Same,
            "different" => Rule::Different,
            _ => Rule::Pattern(text),
        })
    }
}

impl JsonSchema for Rule {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SimilarRule".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "`any`, `same` (as the template), `different`, or a pattern the \
                            attribute's text must match, where `*` is any run of characters, \
                            ignoring case.",
            "type": "string"
        })
    }
}

/// Where to look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimilarScope {
    /// The template's Schematic Sheet.
    Sheet,
    /// Every Schematic Sheet of the Project.
    Project,
    /// The PCB Document.
    Board,
    /// The open Library element the template is part of.
    Element,
}

/// The object to compare with: a UUID, or a Pad of a Footprint instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Template {
    Object(Uuid),
    Pad(PadRef),
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindSimilarParams {
    /// The object to compare with, by UUID, or a Pad of a Footprint instance as
    /// `{footprint, pad}`.
    pub template: Template,
    /// How each attribute must compare, by attribute name: a field of the object schema,
    /// a nested one by dotted path such as `fields.value`, or `type` for the kind.
    /// Attributes left out are `any`, except `type`, which is `same`.
    #[serde(default)]
    pub rules: BTreeMap<String, Rule>,
    pub scope: SimilarScope,
}

/// What `objects.find_similar` found: what `selection.set` takes.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct FindSimilarResult {
    /// The matching objects, the template included when it is one: sheet by sheet in
    /// Document order, then by UUID.
    pub objects: Vec<Uuid>,
    /// The matching Pads of Footprint instances, by Footprint instance then Pad.
    pub pads: Vec<PadRef>,
}

pub struct FindSimilar;

impl Command for FindSimilar {
    const META: Meta = Meta {
        id: "objects.find_similar",
        title: "Find Similar Objects",
        description: "Lists the objects like a template, attribute by attribute: each rule \
                      says an attribute is `any`, the `same` as the template's, `different`, \
                      or matches a `*` pattern. Kind stays the same unless `type` is `any`. \
                      The scope is the template's sheet, the Project's sheets, the board, or \
                      the open Library element. Pads of Footprints can be found; other parts \
                      of a Component or Footprint cannot.",
        category: "query",
        icon: "select_similar",
        kind: Kind::Query,
    };
    type Params = FindSimilarParams;
    type Output = FindSimilarResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FindSimilarParams,
    ) -> Result<FindSimilarResult, CommandError> {
        // Open Library elements live in the workbench while no Project is open.
        find_similar(cx.project()?, &p)
    }
}

/// Where a findable thing lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Home {
    Sheet(Uuid),
    Board,
    Element(Uuid),
}

/// What a findable thing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Id {
    Object(Uuid),
    Pad(PadRef),
}

/// One findable thing and its attributes.
struct Candidate {
    id: Id,
    home: Home,
    attributes: Value,
}

/// [`FindSimilar`]'s work, on `project`.
pub fn find_similar(
    project: &Project,
    p: &FindSimilarParams,
) -> Result<FindSimilarResult, CommandError> {
    let template = template(project, p.template)?;
    let fits = matches!(
        (p.scope, template.home),
        (SimilarScope::Sheet | SimilarScope::Project, Home::Sheet(_))
            | (SimilarScope::Board, Home::Board)
            | (SimilarScope::Element, Home::Element(_))
    );
    if !fits {
        let wanted = match template.home {
            Home::Sheet(_) => "`sheet` or `project`",
            Home::Board => "`board`",
            Home::Element(_) => "`element`",
        };
        return Err(CommandError::invalid_params(
            "/scope",
            format!("the template is not in this scope; use {wanted}"),
        ));
    }
    let mut rules = p.rules.clone();
    rules.entry("type".to_owned()).or_insert(Rule::Same);
    for (name, rule) in &rules {
        if *rule != Rule::Any && attribute(&template.attributes, name).is_none() {
            return Err(CommandError::invalid_params(
                &format!("/rules/{}", pointer_escape(name)),
                format!("the template has no attribute `{name}`"),
            ));
        }
    }
    let candidates = match p.scope {
        SimilarScope::Sheet => match template.home {
            Home::Sheet(sheet) => sheet_candidates(project, sheet),
            _ => Vec::new(),
        },
        SimilarScope::Project => project
            .documents()
            .flat_map(|d| sheet_candidates(project, d.uuid))
            .collect(),
        SimilarScope::Board => board_candidates(project),
        SimilarScope::Element => match template.home {
            Home::Element(element) => element_candidates(project, element),
            _ => Vec::new(),
        },
    };
    let mut out = FindSimilarResult::default();
    for candidate in candidates {
        let like = rules.iter().all(|(name, rule)| {
            matches(
                rule,
                attribute(&template.attributes, name),
                attribute(&candidate.attributes, name),
            )
        });
        if like {
            match candidate.id {
                Id::Object(uuid) => out.objects.push(uuid),
                Id::Pad(pad) => out.pads.push(pad),
            }
        }
    }
    out.pads.sort();
    Ok(out)
}

/// `name` escaped for a JSON Pointer segment.
fn pointer_escape(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}

/// The attribute at dotted path `name`, if the object has it.
fn attribute<'v>(attributes: &'v Value, name: &str) -> Option<&'v Value> {
    if let Some(value) = attributes.get(name) {
        return Some(value);
    }
    name.split('.')
        .try_fold(attributes, |value, key| value.get(key))
}

fn matches(rule: &Rule, template: Option<&Value>, candidate: Option<&Value>) -> bool {
    match rule {
        Rule::Any => true,
        Rule::Same => template == candidate,
        Rule::Different => template != candidate,
        Rule::Pattern(pattern) => candidate.is_some_and(|v| glob(pattern, &text_of(v))),
    }
}

/// An attribute value as a pattern sees it: a string as itself, anything else as JSON.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether `text` matches `pattern`, where `*` is any run of characters, ignoring case.
fn glob(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if p < pattern.len() && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some((sp, st)) = star {
            p = sp + 1;
            t = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// The template as a [`Candidate`], or why it can't be one.
fn template(project: &Project, template: Template) -> Result<Candidate, CommandError> {
    let found = match template {
        Template::Pad(pad) => board_candidates(project)
            .into_iter()
            .find(|c| c.id == Id::Pad(pad)),
        Template::Object(uuid) => {
            let placed = project.object(uuid).map(|object| placed(&object));
            match placed {
                Some(Some(candidate)) => Some(candidate),
                Some(None) => {
                    return Err(CommandError::rejected(format!(
                        "{uuid} is not an object placed on a sheet, the board or a Library \
                         element, so nothing can be similar to it"
                    )));
                }
                None => project.elements().find_map(|(_, e)| {
                    element_parts(e)
                        .into_iter()
                        .find(|c| c.id == Id::Object(uuid))
                }),
            }
        }
    };
    found.ok_or_else(|| match template {
        Template::Object(uuid) => CommandError::not_found(format!("no object {uuid}")),
        Template::Pad(pad) => {
            CommandError::not_found(format!("no Pad {} on Footprint {}", pad.pad, pad.footprint))
        }
    })
}

/// `object` as a [`Candidate`] when it is placed on a sheet or the board.
fn placed(object: &Object) -> Option<Candidate> {
    let home = match object {
        Object::Unit(o) => Home::Sheet(o.document),
        Object::Wire(o) => Home::Sheet(o.document),
        Object::NetLabel(o) => Home::Sheet(o.document),
        Object::NoConnect(o) => Home::Sheet(o.document),
        Object::Port(o) => Home::Sheet(o.document),
        Object::PowerPort(o) => Home::Sheet(o.document),
        Object::SheetText(o) => Home::Sheet(o.document),
        Object::SheetGraphic(o) => Home::Sheet(o.document),
        Object::BoardOutline(_)
        | Object::BoardCutout(_)
        | Object::PcbText(_)
        | Object::PcbGraphic(_)
        | Object::FootprintInstance(_)
        | Object::Track(_)
        | Object::Via(_)
        | Object::Zone(_)
        | Object::Keepout(_)
        | Object::FreePad(_) => Home::Board,
        _ => return None,
    };
    Some(Candidate {
        id: Id::Object(object.uuid()),
        home,
        attributes: serde_json::to_value(object).unwrap_or(Value::Null),
    })
}

/// The placed objects of Schematic Sheet `sheet`, by UUID.
fn sheet_candidates(project: &Project, sheet: Uuid) -> Vec<Candidate> {
    let Some(sheet) = project.schematic_sheet(sheet) else {
        return Vec::new();
    };
    let mut objects = sheet.objects();
    objects.sort_by_key(Object::uuid);
    objects.iter().filter_map(placed).collect()
}

/// The placed objects of the board by UUID, then the Pads of its Footprint instances.
fn board_candidates(project: &Project) -> Vec<Candidate> {
    let Some(pcb) = project.pcb() else {
        return Vec::new();
    };
    let mut objects = pcb.objects();
    objects.sort_by_key(Object::uuid);
    let mut out: Vec<Candidate> = objects.iter().filter_map(placed).collect();
    for instance in pcb.footprints() {
        let Some(copy) = pcb.footprint_copy(instance.footprint) else {
            continue;
        };
        for pad in &copy.footprint.pads {
            let mut attributes = part_attributes("pad", pad);
            if let Value::Object(map) = &mut attributes {
                map.insert("footprint".to_owned(), json!(instance.uuid));
                map.insert("net".to_owned(), json!(instance.pad_net(pad.uuid)));
            }
            out.push(Candidate {
                id: Id::Pad(PadRef {
                    footprint: instance.uuid,
                    pad: pad.uuid,
                }),
                home: Home::Board,
                attributes,
            });
        }
    }
    out
}

/// The parts of the open Library element `element`.
fn element_candidates(project: &Project, element: Uuid) -> Vec<Candidate> {
    project
        .element(element)
        .map(element_parts)
        .unwrap_or_default()
}

/// The parts of `element`, by UUID: a Symbol's Pins and graphics, a Footprint's Pads,
/// graphics and texts. A Part has none.
fn element_parts(element: &LibraryElement) -> Vec<Candidate> {
    let mut out: Vec<(Uuid, Value)> = Vec::new();
    let home = Home::Element(element.uuid());
    match element {
        LibraryElement::Symbol(s) => {
            out.extend(s.pins.iter().map(|x| (x.uuid, part_attributes("pin", x))));
            out.extend(
                s.graphics
                    .iter()
                    .map(|x| (x.uuid, part_attributes("graphic", x))),
            );
        }
        LibraryElement::Footprint(f) => {
            out.extend(f.pads.iter().map(|x| (x.uuid, part_attributes("pad", x))));
            out.extend(
                f.graphics
                    .iter()
                    .map(|x| (x.uuid, part_attributes("graphic", x))),
            );
            out.extend(f.texts.iter().map(|x| (x.uuid, part_attributes("text", x))));
        }
        LibraryElement::Part(_) => {}
    }
    out.sort_by_key(|(uuid, _)| *uuid);
    out.into_iter()
        .map(|(uuid, attributes)| Candidate {
            id: Id::Object(uuid),
            home,
            attributes,
        })
        .collect()
}

/// A part's fields with its kind as `type`.
fn part_attributes(kind: &str, part: &impl Serialize) -> Value {
    let mut map = match serde_json::to_value(part) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    map.insert("type".to_owned(), json!(kind));
    Value::Object(map)
}
