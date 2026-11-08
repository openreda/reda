//! What the Palette lists: its scopes, picked by the query's first character as in VS
//! Code, and the rows each one searches, built once when the Palette opens.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use reda_engine::model::{ComponentRef, DocumentKind, Uuid};
use reda_engine::schematic::connectivity::project_nets;
use reda_engine::schematic::natural_cmp;

use super::{Entry, Shortcuts, entries, score, search};
use crate::shell::Shell;
use crate::tr;

/// What the Palette searches, by the query's first character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    /// No prefix: Documents, Components and Nets; choosing one goes to it.
    GoTo,
    /// `>`: every Command and Preset; choosing one runs it.
    Commands,
    /// `@`: Components; choosing one selects and zooms to it.
    Components,
    /// `#`: Nets; choosing one highlights it.
    Nets,
    /// `:`: Documents; choosing one opens it.
    Documents,
    /// `?`: the prefixes themselves.
    Help,
}

impl Scope {
    /// The scopes that have a prefix, in the order `?` lists them.
    pub const PREFIXED: [Scope; 5] = [
        Scope::Commands,
        Scope::Components,
        Scope::Nets,
        Scope::Documents,
        Scope::Help,
    ];

    /// The scope `query` asks for and the text to search for in it.
    pub fn of(query: &str) -> (Scope, &str) {
        let mut chars = query.chars();
        let scope = chars.next().and_then(|c| {
            Scope::PREFIXED
                .into_iter()
                .find(|s| s.prefix().starts_with(c))
        });
        match scope {
            Some(scope) => (scope, chars.as_str()),
            None => (Scope::GoTo, query),
        }
    }

    /// The character that picks it; empty for [`Scope::GoTo`].
    pub fn prefix(self) -> &'static str {
        match self {
            Scope::GoTo => "",
            Scope::Commands => ">",
            Scope::Components => "@",
            Scope::Nets => "#",
            Scope::Documents => ":",
            Scope::Help => "?",
        }
    }

    /// What it searches, as `?` lists it.
    fn title(self) -> String {
        match self {
            Scope::GoTo => tr!("palette-scope-go-to"),
            Scope::Commands => tr!("palette-scope-commands"),
            Scope::Components => tr!("palette-scope-components"),
            Scope::Nets => tr!("palette-scope-nets"),
            Scope::Documents => tr!("palette-scope-documents"),
            Scope::Help => tr!("palette-scope-help"),
        }
    }
}

/// What choosing a row does.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// Run this Command or Preset id.
    Run(String),
    /// Select and zoom to this Component.
    Component(ComponentRef),
    /// Highlight the Net of this name.
    Net(String),
    /// Open this Document.
    Document(Uuid),
    /// Fill in this scope's prefix.
    Prefix(Scope),
}

/// One row of the Palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub target: Target,
    pub title: String,
    pub icon: String,
    /// What the right of the row shows: a Command's shortcut, a Component's value, a
    /// Document's kind.
    pub detail: Option<String>,
    /// The hover text.
    pub description: String,
    /// Why it cannot be chosen now, or `None` when it can.
    pub unavailable: Option<String>,
    /// Shown because it was chosen lately.
    pub recent: bool,
}

/// Every row the Palette can show, as things stood when it opened.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    commands: Vec<Entry>,
    /// Project order.
    documents: Vec<Row>,
    /// Natural order of Designator.
    components: Vec<Row>,
    /// Named Nets, in natural order of name.
    nets: Vec<Row>,
}

impl Sources {
    /// Every Command and Preset in `shell`'s registry, and the open Project's Documents,
    /// Components and named Nets.
    pub fn of(shell: &Shell, shortcut: Shortcuts<'_>) -> Sources {
        let mut sources = Sources {
            commands: entries(shell, shortcut),
            ..Sources::default()
        };
        let Some(project) = shell.engine().project() else {
            return sources;
        };
        for d in project.documents() {
            let (icon, kind) = match d.kind {
                DocumentKind::Schematic => ("document_schematic", tr!("palette-kind-schematic")),
                DocumentKind::Pcb => ("document_pcb", tr!("palette-kind-pcb")),
            };
            sources.documents.push(row(
                Target::Document(d.uuid),
                d.title.clone(),
                icon,
                Some(kind),
                d.file_name(),
            ));
        }
        let mut components: BTreeMap<ComponentRef, (String, String)> = BTreeMap::new();
        for d in project.documents() {
            for unit in project
                .schematic_sheet(d.uuid)
                .into_iter()
                .flat_map(|s| s.units())
            {
                components
                    .entry(unit.component.clone())
                    .or_insert_with(|| (unit.designator.clone(), unit.fields.value.clone()));
            }
        }
        let mut components: Vec<_> = components.into_iter().collect();
        components.sort_by(|(_, (a, _)), (_, (b, _))| natural_cmp(a, b));
        for (component, (designator, value)) in components {
            let detail = (!value.is_empty()).then(|| value.clone());
            sources.components.push(row(
                Target::Component(component),
                designator,
                "component_place",
                detail,
                value,
            ));
        }
        for net in project_nets(project) {
            let Some(name) = net.name else { continue };
            if net.objects.is_empty() || sources.nets.iter().any(|r| r.title == name) {
                continue;
            }
            let kind = tr!("palette-kind-net");
            sources.nets.push(row(
                Target::Net(name.clone()),
                name,
                "net",
                Some(kind),
                String::new(),
            ));
        }
        sources
    }

    /// The rows `query` finds, best first; `recent` are the targets chosen lately, most
    /// recent first, which an empty query shows before the Documents.
    pub fn rows(&self, query: &str, recent: &[Target]) -> Vec<Row> {
        let (scope, text) = Scope::of(query);
        match scope {
            Scope::Commands => search(&self.commands, text)
                .into_iter()
                .map(command_row)
                .collect(),
            Scope::Components => ranked(text, [&self.components]),
            Scope::Nets => ranked(text, [&self.nets]),
            Scope::Documents => ranked(text, [&self.documents]),
            Scope::Help => Scope::PREFIXED
                .into_iter()
                .map(|s| {
                    row(
                        Target::Prefix(s),
                        s.prefix().to_owned(),
                        "search",
                        Some(s.title()),
                        String::new(),
                    )
                })
                .collect(),
            Scope::GoTo if text.trim().is_empty() => {
                let mut rows: Vec<Row> = recent
                    .iter()
                    .filter_map(|t| self.find(t))
                    .map(|mut r| {
                        r.recent = true;
                        r
                    })
                    .collect();
                for d in &self.documents {
                    if !rows.iter().any(|r| r.target == d.target) {
                        rows.push(d.clone());
                    }
                }
                rows
            }
            Scope::GoTo => ranked(text, [&self.documents, &self.components, &self.nets]),
        }
    }

    /// The row of `target`, if it is still there.
    fn find(&self, target: &Target) -> Option<Row> {
        if let Target::Run(id) = target {
            return self.commands.iter().find(|e| &e.id == id).map(command_row);
        }
        [&self.documents, &self.components, &self.nets]
            .into_iter()
            .flatten()
            .find(|r| &r.target == target)
            .cloned()
    }
}

fn row(
    target: Target,
    title: String,
    icon: &str,
    detail: Option<String>,
    description: String,
) -> Row {
    Row {
        target,
        title,
        icon: icon.to_owned(),
        detail,
        description,
        unavailable: None,
        recent: false,
    }
}

fn command_row(e: &Entry) -> Row {
    Row {
        target: Target::Run(e.id.clone()),
        title: e.title.clone(),
        icon: e.icon.clone(),
        detail: e.shortcut.clone(),
        description: e.description.clone(),
        unavailable: e.unavailable.clone(),
        recent: false,
    }
}

/// The rows of `lists` whose title matches `text`, best first; equals keep list order.
fn ranked<const N: usize>(text: &str, lists: [&Vec<Row>; N]) -> Vec<Row> {
    let mut found: Vec<(i32, &Row)> = lists
        .into_iter()
        .flatten()
        .filter_map(|r| Some((score(text, &r.title)?, r)))
        .collect();
    found.sort_by_key(|(s, _)| Reverse(*s));
    found.into_iter().map(|(_, r)| r.clone()).collect()
}
