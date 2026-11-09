//! The Find Panel: searches the schematic (`schematic.find`) by Designator, value, Net
//! name or any text, as plain text, `*`/`?` wildcards or a regular expression, on this
//! sheet, the Project or the selection. The results list each match's kind, text and
//! sheet; picking one opens its sheet, selects it and zooms to it, and F3 and Shift+F3
//! (`schematic.find.step`) step through the list. **Mask Matching** masks everything but
//! the matches.
//!
//! What the Panel holds is view state ([`FindPanel`]), so the step keys reach it from any
//! focus.

use egui::{Key, Role, ScrollArea, TextEdit, Ui, WidgetInfo};
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Preset};
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// One match.
#[derive(Clone, Debug, PartialEq)]
pub struct FindRow {
    /// What kind of match, as the list shows it: "Designator", "Net Label"….
    pub kind: String,
    /// The matching text.
    pub text: String,
    /// The sheet's title.
    pub sheet: String,
    /// The sheet it is on.
    pub document: Uuid,
    /// What going to it selects: the Unit, the object holding the text, or the objects on
    /// the Net on that sheet.
    pub objects: Vec<Uuid>,
}

impl FindRow {
    /// The row's label: "R2 (Designator) on Sheet 2".
    pub fn label(&self) -> String {
        tr!(
            "find-row",
            text = self.text.clone(),
            field = self.kind.clone(),
            sheet = self.sheet.clone()
        )
    }
}

/// How the text is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Syntax {
    #[default]
    Plain,
    Wildcard,
    Regex,
}

/// Where Find looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scope {
    /// The active sheet.
    Sheet,
    /// Every sheet.
    #[default]
    Project,
    /// The selected objects.
    Selection,
}

/// What Find looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kinds {
    pub designators: bool,
    pub values: bool,
    pub nets: bool,
    /// Net Label, Port and Power Port names and free text.
    pub text: bool,
}

impl Default for Kinds {
    fn default() -> Kinds {
        Kinds {
            designators: true,
            values: true,
            nets: true,
            text: true,
        }
    }
}

/// The Find Panel's view state.
#[derive(Clone, Debug, Default)]
pub struct FindPanel {
    pub text: String,
    pub kinds: Kinds,
    pub match_case: bool,
    pub whole_word: bool,
    pub syntax: Syntax,
    pub scope: Scope,
    /// Mask Matching: everything but the matches is masked.
    pub mask: bool,
    results: Vec<FindRow>,
    current: Option<usize>,
    searched: bool,
    error: Option<String>,
    /// The text field takes the keyboard on the next frame (Mod+F).
    focus: bool,
}

impl FindPanel {
    /// The matches of the last search.
    pub fn results(&self) -> &[FindRow] {
        &self.results
    }

    /// The row shown last.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// Whether a search has run (so an empty list means nothing was found).
    pub fn searched(&self) -> bool {
        self.searched
    }

    /// Why the last search failed, such as an invalid regular expression.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The `schematic.find` params for this state, with the Shell's active sheet and
    /// selection for the scope.
    pub fn params(&self, shell: &Shell) -> Value {
        let mut fields = Vec::new();
        for (on, field) in [
            (self.kinds.designators, "designator"),
            (self.kinds.values, "value"),
            (self.kinds.nets, "net"),
            (self.kinds.text, "text"),
        ] {
            if on {
                fields.push(field);
            }
        }
        let syntax = match self.syntax {
            Syntax::Plain => "plain",
            Syntax::Wildcard => "wildcard",
            Syntax::Regex => "regex",
        };
        let mut params = json!({
            "text": self.text,
            "fields": fields,
            "match_case": self.match_case,
            "whole_word": self.whole_word,
            "syntax": syntax,
        });
        match self.scope {
            Scope::Project => {}
            Scope::Sheet => {
                if let Some(sheet) = shell.active_sheet() {
                    params["document"] = json!(sheet);
                }
            }
            Scope::Selection => params["objects"] = json!(shell.selection().objects),
        }
        params
    }
}

/// The Panel's state.
pub fn state(shell: &Shell) -> FindPanel {
    shell
        .engine()
        .view()
        .get::<FindPanel>()
        .cloned()
        .unwrap_or_default()
}

fn put(shell: &mut Shell, panel: FindPanel) {
    if let Some(state) = shell.engine.view_mut().get_mut::<FindPanel>() {
        *state = panel;
    }
}

/// Changes the Panel's options (as its widgets do).
pub fn edit(shell: &mut Shell, change: impl FnOnce(&mut FindPanel)) {
    let mut panel = state(shell);
    change(&mut panel);
    put(shell, panel);
}

/// Mod+F: the Panel shows and its text field takes the keyboard.
pub fn open(shell: &mut Shell) {
    let _ = shell.run_reporting("panel.show:find", json!({}));
    edit(shell, |p| p.focus = true);
}

/// Runs the search, shows its first match, and masks the rest with Mask Matching.
pub fn search(shell: &mut Shell) {
    let mut panel = state(shell);
    panel.results.clear();
    panel.current = None;
    panel.error = None;
    panel.searched = false;
    if panel.text.trim().is_empty() {
        put(shell, panel);
        return;
    }
    let out = match shell.run("schematic.find", panel.params(shell)) {
        Ok(out) => out,
        Err(e) => {
            panel.error = Some(e.message);
            put(shell, panel);
            return;
        }
    };
    panel.searched = true;
    panel.results = rows(shell, &out);
    let first = (!panel.results.is_empty()).then_some(0);
    let mask = panel.mask;
    let matched: Vec<Uuid> = panel
        .results
        .iter()
        .flat_map(|r| r.objects.iter().copied())
        .collect();
    put(shell, panel);
    if mask && !matched.is_empty() {
        let _ = shell.run_reporting("view.highlight", json!({"objects": matched, "mask": true}));
    }
    if let Some(first) = first {
        go(shell, first);
    }
}

fn rows(shell: &Shell, out: &Value) -> Vec<FindRow> {
    let project = shell.engine().project();
    let title = |document: Uuid| {
        project
            .and_then(|p| p.document(document))
            .map(|d| d.title.clone())
            .unwrap_or_default()
    };
    let mut rows = Vec::new();
    for m in out["matches"].as_array().into_iter().flatten() {
        let Some(document) = uuid(&m["document"]) else {
            continue;
        };
        let kind = match (m["field"].as_str(), m["kind"].as_str()) {
            (Some("designator"), _) => tr!("find-field-designator"),
            (Some("value"), _) => tr!("find-field-value"),
            (Some("net"), _) => tr!("find-field-net"),
            (_, Some("net_label")) => tr!("find-kind-net-label"),
            (_, Some("port")) => tr!("find-kind-port"),
            (_, Some("power_port")) => tr!("find-kind-power-port"),
            _ => tr!("find-kind-text"),
        };
        let objects: Vec<Uuid> = match uuid(&m["unit"]) {
            Some(unit) => vec![unit],
            None => m["objects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(uuid)
                .collect(),
        };
        rows.push(FindRow {
            kind,
            text: m["text"].as_str().unwrap_or_default().to_owned(),
            sheet: title(document),
            document,
            objects,
        });
    }
    rows
}

/// Shows match `index`: opens its sheet, selects it and zooms to it.
pub fn go(shell: &mut Shell, index: usize) {
    let mut panel = state(shell);
    let Some(row) = panel.results.get(index).cloned() else {
        return;
    };
    panel.current = Some(index);
    put(shell, panel);
    shell.reveal(row.document, &row.objects);
}

/// F3 (`forward`) or Shift+F3: shows the next or previous match, wrapping around.
pub fn step(shell: &mut Shell, forward: bool) {
    let panel = state(shell);
    let n = panel.results.len();
    if n == 0 {
        return;
    }
    let next = match (panel.current, forward) {
        (None, true) => 0,
        (None, false) => n - 1,
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
    };
    go(shell, next);
}

/// Draws the Panel.
pub fn show(ui: &mut Ui, shell: &mut Shell) {
    let mut panel = state(shell);
    let before = (panel.kinds, panel.match_case, panel.whole_word, panel.syntax);
    let before = (before, panel.scope, panel.mask);
    let mut search_now = false;
    let mut go_to = None;
    ui.horizontal(|ui| {
        let field = ui.add(
            TextEdit::singleline(&mut panel.text)
                .hint_text(tr!("find-hint"))
                .desired_width((ui.available_width() - 50.0).max(80.0)),
        );
        field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, tr!("find-text")));
        if panel.focus {
            field.request_focus();
            panel.focus = false;
        }
        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            search_now = true;
        }
        if ui.button(tr!("find-search")).clicked() {
            search_now = true;
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut panel.kinds.designators, tr!("find-kinds-designators"));
        ui.checkbox(&mut panel.kinds.values, tr!("find-kinds-values"));
        ui.checkbox(&mut panel.kinds.nets, tr!("find-kinds-nets"));
        ui.checkbox(&mut panel.kinds.text, tr!("find-kinds-text"));
    });
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut panel.match_case, tr!("find-match-case"));
        ui.checkbox(&mut panel.whole_word, tr!("find-whole-word"));
    });
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut panel.syntax, Syntax::Plain, tr!("find-syntax-plain"));
        ui.selectable_value(
            &mut panel.syntax,
            Syntax::Wildcard,
            tr!("find-syntax-wildcard"),
        );
        ui.selectable_value(&mut panel.syntax, Syntax::Regex, tr!("find-syntax-regex"));
    });
    ui.horizontal_wrapped(|ui| {
        ui.weak(tr!("find-scope"));
        ui.selectable_value(&mut panel.scope, Scope::Sheet, tr!("find-scope-sheet"));
        ui.selectable_value(&mut panel.scope, Scope::Project, tr!("find-scope-project"));
        ui.selectable_value(
            &mut panel.scope,
            Scope::Selection,
            tr!("find-scope-selection"),
        );
    });
    ui.checkbox(&mut panel.mask, tr!("find-mask"));
    if let Some(error) = &panel.error {
        ui.colored_label(ui.visuals().error_fg_color, error);
    } else if panel.searched && panel.results.is_empty() {
        ui.weak(tr!("find-nothing"));
    } else if panel.searched {
        ui.weak(tr!("find-count", count = panel.results.len()));
    }
    ui.separator();
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (i, row) in panel.results.iter().enumerate() {
            let label = row.label();
            let response = ui.selectable_label(panel.current == Some(i), &label);
            if response.clicked() {
                go_to = Some(i);
            }
        }
    });
    let after = (panel.kinds, panel.match_case, panel.whole_word, panel.syntax);
    // Changing an option searches again once there is a search to redo.
    let changed = (after, panel.scope, panel.mask) != before;
    let searched = panel.searched;
    put(shell, panel);
    if search_now || (changed && searched) {
        search(shell);
    } else if let Some(i) = go_to {
        go(shell, i);
    }
}

fn uuid(v: &Value) -> Option<Uuid> {
    v.as_str()?.parse().ok()
}

/// Which way F3 steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Next,
    Previous,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepParams {
    /// Go to the next match in the Find Panel's list, or the previous one.
    direction: Direction,
}

/// What `schematic.find.step` asks of the Shell, which goes to that match.
#[derive(Serialize, JsonSchema)]
pub struct Stepped {
    pub direction: Direction,
}

pub(crate) struct FindStep;

impl Command for FindStep {
    const META: Meta = Meta {
        id: "schematic.find.step",
        title: "Find Next or Previous",
        description: "Goes to the next or previous match in the Find Panel's list: opens \
                      its sheet, selects it and zooms to it. It wraps around at either end. \
                      View state only.",
        category: "schematic",
        icon: "find",
        kind: Kind::View,
    };
    type Params = StepParams;
    type Output = Stepped;

    fn presets(&self) -> Vec<Preset> {
        vec![
            Preset::new("next", "Find Next", "find", json!({"direction": "next"})),
            Preset::new(
                "previous",
                "Find Previous",
                "find",
                json!({"direction": "previous"}),
            ),
        ]
    }

    fn run(&self, _cx: &mut Context<'_>, p: StepParams) -> Result<Stepped, CommandError> {
        Ok(Stepped {
            direction: p.direction,
        })
    }
}
