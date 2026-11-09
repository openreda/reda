//! Finding objects across sheets: `schematic.find` searches by Designator, value, Net
//! name or any sheet text, as plain text, `*`/`?` wildcards or a regular expression, on
//! a sheet, the Project or a selection.

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
    /// All text: Net Label, Port and Power Port names and free text.
    Text,
}

/// How a find query's text is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindSyntax {
    /// The text itself.
    #[default]
    Plain,
    /// `*` matches any run of characters and `?` any one character, over the whole
    /// text, always ignoring case.
    Wildcard,
    /// A regular expression, matching anywhere unless anchored.
    Regex,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindParams {
    /// The text to look for.
    pub text: String,
    /// What to search; Designators, values and Net names by default.
    pub fields: Option<Vec<FindField>>,
    /// Match the whole text rather than any part of it.
    #[serde(default)]
    pub exact: bool,
    /// Match case; case does not matter by default (and never for wildcards).
    #[serde(default)]
    pub match_case: bool,
    /// Match only whole words: the text with no letter, digit or `_` on either side.
    #[serde(default)]
    pub whole_word: bool,
    /// How `text` is read: plain text by default.
    #[serde(default)]
    pub syntax: FindSyntax,
    /// Search only this Schematic Sheet; every sheet by default.
    pub document: Option<Uuid>,
    /// Search only these objects (the selection): a Net matches when one of its objects
    /// is among them.
    pub objects: Option<Vec<Uuid>>,
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
    /// For a text match, what holds the text: `net_label`, `port`, `power_port` or
    /// `text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
    /// The objects that carry the matched Net on this sheet, for Net matches; the one
    /// object, for text matches.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<Uuid>,
}

pub struct Find;

impl Command for Find {
    const META: Meta = Meta {
        id: "schematic.find",
        title: "Find",
        description: "Finds objects on every Schematic Sheet (or one, or among given \
                      objects) whose Designator, value, Net name or text contains the text \
                      (or equals it, with `exact`), ignoring case unless `match_case`. \
                      `whole_word` matches whole words; `syntax` reads the text as `*`/`?` \
                      wildcards (the whole text, ignoring case) or a regular expression. A \
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
            .clone()
            .unwrap_or_else(|| vec![FindField::Designator, FindField::Value, FindField::Net]);
        let matcher = Matcher::with(&p.text, &options(&p))
            .map_err(|e| CommandError::invalid_params("/text", e))?;
        let project = cx.project()?;
        if let Some(document) = p.document {
            crate::sheet(project, document)?;
        }
        let mut matches = find(project, &matcher, &fields);
        if let Some(document) = p.document {
            matches.retain(|m| m.document == document);
        }
        if let Some(objects) = &p.objects {
            matches.retain(|m| {
                m.unit.iter().chain(&m.objects).any(|o| objects.contains(o))
            });
        }
        Ok(FindResult { matches })
    }
}

fn options(p: &FindParams) -> MatchOptions {
    MatchOptions {
        exact: p.exact,
        match_case: p.match_case,
        whole_word: p.whole_word,
        syntax: p.syntax,
    }
}

/// How a [`Matcher`] compares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchOptions {
    pub exact: bool,
    pub match_case: bool,
    pub whole_word: bool,
    pub syntax: FindSyntax,
}

/// Compares text with a query.
pub struct Matcher {
    regex: regex_lite::Regex,
}

impl Matcher {
    /// Plain text, ignoring case: the whole text with `exact`, else any part of it.
    pub fn new(text: &str, exact: bool) -> Matcher {
        let options = MatchOptions {
            exact,
            ..MatchOptions::default()
        };
        Matcher::with(text, &options).expect("escaped text always compiles")
    }

    /// A query read with `options`; the error says why a regular expression is invalid.
    pub fn with(text: &str, options: &MatchOptions) -> Result<Matcher, String> {
        let body = match options.syntax {
            FindSyntax::Plain => regex_lite::escape(text),
            FindSyntax::Regex => text.to_owned(),
            FindSyntax::Wildcard => {
                let mut out = String::from("^(?:");
                for c in text.chars() {
                    match c {
                        '*' => out.push_str(".*"),
                        '?' => out.push('.'),
                        c => out.push_str(&regex_lite::escape(&c.to_string())),
                    }
                }
                out.push_str(")$");
                out
            }
        };
        let mut pattern = format!("(?:{body})");
        if options.whole_word {
            pattern = format!(r"(?:^|[^\w])({pattern})(?:[^\w]|$)");
        }
        if options.exact {
            pattern = format!("^{pattern}$");
        }
        let fold = !options.match_case || options.syntax == FindSyntax::Wildcard;
        if fold {
            pattern = format!("(?i){pattern}");
        }
        regex_lite::Regex::new(&pattern)
            .map(|regex| Matcher { regex })
            .map_err(|e| e.to_string())
    }

    /// Whether `text`, as it displays, matches.
    pub fn matches(&self, text: &str) -> bool {
        self.regex.is_match(&reda_model::text::display(text))
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
        if fields.contains(&FindField::Text) {
            out.extend(text_matches(sheet, matcher));
        }
    }
    out
}

/// The Net Labels, Ports, Power Ports and free texts on `sheet` whose text matches, in
/// that order of kind, each in natural order.
fn text_matches(sheet: &reda_model::SchematicSheet, matcher: &Matcher) -> Vec<FindMatch> {
    let mut found: Vec<(usize, &str, Uuid, &'static str)> = Vec::new();
    found.extend(sheet.net_labels().map(|o| (0, o.name.as_str(), o.uuid, "net_label")));
    found.extend(sheet.ports().map(|o| (1, o.name.as_str(), o.uuid, "port")));
    found.extend(sheet.power_ports().map(|o| (2, o.name.as_str(), o.uuid, "power_port")));
    found.extend(sheet.texts().map(|o| (3, o.text.as_str(), o.uuid, "text")));
    found.retain(|(_, text, _, _)| matcher.matches(text));
    found.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| natural_cmp(a.1, b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    found
        .into_iter()
        .map(|(_, text, uuid, kind)| FindMatch {
            field: FindField::Text,
            text: text.to_owned(),
            document: sheet.uuid,
            unit: None,
            component: None,
            kind: Some(kind),
            objects: vec![uuid],
        })
        .collect()
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
        kind: None,
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
                    kind: None,
                    objects: objects.into_iter().collect(),
                }
            })
        })
        .collect()
}
