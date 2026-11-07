//! Finding objects across sheets: `schematic.find` searches by Designator, value or Net
//! name.

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::{ComponentRef, Project, UnitSuffix, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::natural::natural_cmp;

/// What a find query compares the text with.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FindField {
    Designator,
    Value,
    /// The name of a Net.
    Net,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindParams {
    /// The text to look for. Case does not matter.
    pub text: String,
    /// What to search; every field by default.
    pub fields: Option<Vec<FindField>>,
    /// Match the whole text rather than any part of it.
    #[serde(default)]
    pub exact: bool,
}

/// What a find query found.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FindResult {
    /// Sheet by sheet in Document order; on each sheet the Designator matches, then the
    /// value matches, then the Net matches, each in natural order.
    pub matches: Vec<FindMatch>,
}

/// One found object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FindMatch {
    /// What matched.
    pub field: FindField,
    /// The whole matching text, such as the Designator.
    pub text: String,
    /// The sheet the object is on.
    pub document: Uuid,
    /// The matched Unit, for Designator and value matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<Uuid>,
    /// The matched Unit's Component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<Uuid>,
    /// The objects that carry the matched Net on this sheet, for Net matches.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<Uuid>,
}

pub struct Find;

impl Command for Find {
    const META: Meta = Meta {
        id: "schematic.find",
        title: "Find",
        description: "Finds objects on every Schematic Sheet whose Designator, value or Net \
                      name contains the text (or equals it, with `exact`), ignoring case. A \
                      Designator also matches with its Unit suffix, as `U1A` or `U1:1`.",
        category: "schematic",
        icon: "find",
        kind: Kind::Query,
    };
    type Params = FindParams;
    type Output = FindResult;

    fn run(&self, cx: &mut Context<'_>, p: FindParams) -> Result<FindResult, CommandError> {
        if p.text.is_empty() {
            return Err(CommandError::invalid_params("/text", "the text is empty"));
        }
        let fields = p
            .fields
            .unwrap_or_else(|| vec![FindField::Designator, FindField::Value, FindField::Net]);
        let matcher = Matcher::new(&p.text, p.exact);
        Ok(FindResult {
            matches: find(cx.project()?, &matcher, &fields),
        })
    }
}

/// Compares text with a query, ignoring case.
pub struct Matcher {
    needle: String,
    exact: bool,
}

impl Matcher {
    pub fn new(text: &str, exact: bool) -> Matcher {
        Matcher {
            needle: text.to_lowercase(),
            exact,
        }
    }

    /// Whether `text`, as it displays, matches.
    pub fn matches(&self, text: &str) -> bool {
        let text = reda_model::text::display(text).to_lowercase();
        if self.exact {
            text == self.needle
        } else {
            text.contains(&self.needle)
        }
    }
}

/// Every match on every Schematic Sheet, in [`FindResult::matches`] order.
pub fn find(project: &Project, matcher: &Matcher, fields: &[FindField]) -> Vec<FindMatch> {
    let mut out = Vec::new();
    for sheet in project
        .documents()
        .filter_map(|d| project.schematic_sheet(d.uuid))
    {
        let mut found = Vec::new();
        for unit in sheet.units() {
            // A Designator matches with the Unit suffix after it too, in either style.
            let symbol = sheet.part(unit.part).map(|e| &e.symbol);
            let designators: Vec<String> = std::iter::once(unit.designator.clone())
                .chain(symbol.into_iter().flat_map(|symbol| {
                    [UnitSuffix::Alpha, UnitSuffix::Numeric]
                        .into_iter()
                        .map(|style| style.of(symbol, unit.unit))
                        .filter(|suffix| !suffix.is_empty())
                        .map(|suffix| format!("{}{suffix}", unit.designator))
                }))
                .collect();
            let mut hit = |field: FindField, text: &str| {
                if fields.contains(&field) && matcher.matches(text) {
                    found.push((
                        field,
                        unit.designator.as_str(),
                        unit_match(field, text, sheet.uuid, unit.uuid, &unit.component),
                    ));
                }
            };
            if let Some(text) = designators.iter().find(|d| matcher.matches(d)) {
                hit(FindField::Designator, text);
            }
            hit(FindField::Value, &unit.fields.value);
        }
        found.sort_by(|(fa, da, a), (fb, db, b)| {
            fa.cmp(fb)
                .then_with(|| natural_cmp(da, db))
                .then_with(|| a.unit.cmp(&b.unit))
        });
        out.extend(found.into_iter().map(|(_, _, m)| m));
        if fields.contains(&FindField::Net) {
            out.extend(net_matches(project, sheet.uuid, matcher));
        }
    }
    out
}

fn unit_match(
    field: FindField,
    text: &str,
    document: Uuid,
    unit: Uuid,
    component: &ComponentRef,
) -> FindMatch {
    FindMatch {
        field,
        text: text.to_owned(),
        document,
        unit: Some(unit),
        component: Some(component.uuid),
        objects: vec![],
    }
}

/// The Nets on `sheet` whose name matches, one [`FindMatch`] per Net with the objects
/// on it (Wires, Net Labels, No-Connect Markers and the Units with a Pin on it), in
/// natural order of the Net name. Nets are derived by [`crate::connectivity`]. Unnamed
/// Nets never match, nor do Nets that are only one unwired Pin (every unconnected Pin is
/// a Net of its own, and finding those would bury the real Nets).
fn net_matches(project: &Project, sheet: Uuid, matcher: &Matcher) -> Vec<FindMatch> {
    let Some(conn) = crate::connectivity::sheet_connectivity(project, sheet) else {
        return Vec::new();
    };
    // Nets come named first, in natural order of name.
    conn.nets
        .iter()
        .filter_map(|net| {
            let name = net.name.as_deref()?;
            let lone_pin = net.pins.len() == 1 && net.objects().count() == 1;
            if lone_pin {
                return None;
            }
            matcher.matches(name).then(|| {
                let objects: std::collections::BTreeSet<Uuid> = net.objects().collect();
                FindMatch {
                    field: FindField::Net,
                    text: name.to_owned(),
                    document: sheet,
                    unit: None,
                    component: None,
                    objects: objects.into_iter().collect(),
                }
            })
        })
        .collect()
}
