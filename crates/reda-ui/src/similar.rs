//! The Select Similar window (docs/spec/v0.1/editing.md#select-similar): non-modal, because
//! the result is judged on the canvas. It compares with one template object, picked with
//! Shift+F then a click (`selection.similar.pick`) or opened from the context menu
//! (`selection.similar.window`).
//!
//! - **Rows** are generated from the object schema
//!   ([`describe_template`](reda_engine::commands::similar::describe_template)) and grouped
//!   as Kind, Object Specific and Graphical. Kind starts at Same and the rest at Any; a
//!   rule is Any, Same, Different or a `*` pattern.
//! - **Options:** Select (on), Zoom (off), Clear Existing (on), Normal, Dim or Mask (Mask),
//!   Open Properties (off). **Apply** tests the result without closing; OK applies and
//!   closes.
//! - **Selection Filter:** the window won't open for a filtered kind, and filtered kinds
//!   are left out of the result.
//! - **Scope:** the sheet or the Project; the board; or the current Library element.
//!
//! The window is view state ([`SimilarState`]) so the Commands, the canvases and the tests
//! drive it the same way; [`show`] draws it.

use egui::Id;
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use reda_engine::commands::similar::{AttributeGroup, SimilarScope, Template, describe_template};
use reda_engine::model::{Object, PadRef, Scope, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::shell::Shell;
use crate::tr;
use crate::working_state::FilterKind;

/// How a row's attribute must compare with the template's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rule {
    Any,
    Same,
    Different,
    /// Matches this `*` pattern, ignoring case.
    Pattern(String),
}

impl Rule {
    /// The rule as `objects.find_similar` takes it.
    fn param(&self) -> Value {
        match self {
            Rule::Any => json!("any"),
            Rule::Same => json!("same"),
            Rule::Different => json!("different"),
            Rule::Pattern(p) => json!(p),
        }
    }
}

/// One attribute row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The attribute, as `rules` names it (`fields.value`).
    pub name: String,
    /// The template's value, as text.
    pub value: String,
    pub group: AttributeGroup,
    pub rule: Rule,
}

/// What the result looks like on the canvas besides being selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// No highlight.
    Normal,
    /// Everything else dims, and can still be picked.
    Dim,
    /// Everything else is masked: greyed, and can't be hit.
    Mask,
}

/// The open window.
#[derive(Clone, Debug, PartialEq)]
pub struct SimilarWindow {
    /// The canvas the template is on.
    pub document: Uuid,
    pub template: Template,
    pub title: String,
    pub rows: Vec<Row>,
    /// The scopes the template can search, narrowest first.
    pub scopes: Vec<SimilarScope>,
    pub scope: SimilarScope,
    pub select: bool,
    pub zoom: bool,
    pub clear_existing: bool,
    pub show: Show,
    pub open_properties: bool,
    /// How many the last Apply found.
    pub found: Option<usize>,
    pub error: Option<String>,
}

impl SimilarWindow {
    /// Sets the rule of row `name`.
    pub fn set_rule(&mut self, name: &str, rule: Rule) {
        if let Some(row) = self.rows.iter_mut().find(|r| r.name == name) {
            row.rule = rule;
        }
    }

    /// The row `name`.
    pub fn row(&self, name: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.name == name)
    }

    /// The `objects.find_similar` params the rows and scope make.
    pub fn params(&self) -> Value {
        let rules: Map<String, Value> = self
            .rows
            .iter()
            // Kind is `same` unless said otherwise, so Any is written out for it.
            .filter(|r| r.rule != Rule::Any || r.name == "type")
            .map(|r| (r.name.clone(), r.rule.param()))
            .collect();
        json!({"template": self.template, "rules": rules, "scope": self.scope})
    }
}

/// The Select Similar window and Shift+F's pending pick. View state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimilarState {
    pub window: Option<SimilarWindow>,
    /// Shift+F: the next click on a canvas picks the template.
    pub picking: bool,
    /// A Command asked to open the window on this template.
    opening: Option<(Uuid, Template)>,
}

/// Adds the Commands. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(Pick);
    registry.add(OpenWindow);
}

/// Done.
#[derive(Serialize, JsonSchema)]
pub struct Done {}

/// `selection.similar.pick` (Shift+F): the next click on a canvas picks the template and
/// opens the Select Similar window on it.
struct Pick;

impl Command for Pick {
    const META: Meta = Meta {
        id: "selection.similar.pick",
        title: "Select Similar…",
        description: "Waits for a click on a canvas: the object clicked is the template the \
                      Select Similar window compares with. A click on nothing cancels.",
        category: "selection",
        icon: "select_similar",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Done, CommandError> {
        cx.view::<SimilarState>()?.picking = true;
        Ok(Done {})
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenParams {
    /// Objects, the first of which is the template.
    #[serde(default)]
    objects: Vec<Uuid>,
    /// Pads of Footprint instances: the first is the template when `objects` is empty.
    #[serde(default)]
    pads: Vec<PadRef>,
    /// The canvas the template is on; found from the object when left out.
    #[serde(default)]
    document: Option<Uuid>,
}

/// `selection.similar.window`: opens the Select Similar window on an object, as the
/// context menu does for what was right-clicked.
struct OpenWindow;

impl Command for OpenWindow {
    const META: Meta = Meta {
        id: "selection.similar.window",
        title: "Select Similar…",
        description: "Opens the non-modal Select Similar window comparing with the first \
                      given object, or Pad: one row per attribute of its schema, each Any, \
                      Same, Different or a `*` pattern. It won't open for a kind the \
                      Selection Filter leaves out.",
        category: "selection",
        icon: "select_similar",
        kind: Kind::View,
    };
    type Params = OpenParams;
    type Output = Done;

    fn run(&self, cx: &mut Context<'_>, p: OpenParams) -> Result<Done, CommandError> {
        let template = match (p.objects.first(), p.pads.first()) {
            (Some(o), _) => Template::Object(*o),
            (None, Some(pad)) => Template::Pad(*pad),
            (None, None) => return Err(CommandError::unavailable("nothing is selected")),
        };
        let project = cx.project()?;
        let home = match template {
            Template::Object(o) => project.object(o).and_then(|o| match o.scope() {
                Scope::Document(d) => Some(d),
                _ => None,
            }),
            Template::Pad(_) => project.pcb().map(|p| p.uuid),
        };
        let document = match p.document.or(home) {
            Some(d) => d,
            None => cx
                .view::<crate::layout::Layout>()?
                .active_document()
                .ok_or_else(|| CommandError::unavailable("no Document is active"))?,
        };
        // The Shell opens it after the run: it checks the Selection Filter.
        cx.view::<SimilarState>()?.opening = Some((document, template));
        Ok(Done {})
    }
}

/// Opens the window the last Command asked for; the Shell calls it after each run.
/// Returns why it could not open, if it couldn't.
pub fn after_run(shell: &mut Shell) -> Option<String> {
    let opening = state_mut(shell)?.opening.take();
    let (document, template) = opening?;
    open(shell, document, template).err()
}

fn state_mut(shell: &mut Shell) -> Option<&mut SimilarState> {
    shell.engine_mut().view_mut().get_mut::<SimilarState>()
}

/// The window, while open.
pub fn window(shell: &Shell) -> Option<&SimilarWindow> {
    shell.engine().view().get::<SimilarState>()?.window.as_ref()
}

/// The window, to change its rows and options.
pub fn window_mut(shell: &mut Shell) -> Option<&mut SimilarWindow> {
    state_mut(shell)?.window.as_mut()
}

/// Whether Shift+F waits for a click.
pub fn picking(shell: &Shell) -> bool {
    shell
        .engine()
        .view()
        .get::<SimilarState>()
        .is_some_and(|s| s.picking)
}

/// Closes the window.
pub fn close(shell: &mut Shell) {
    if let Some(state) = state_mut(shell) {
        state.window = None;
    }
}

/// A click while Shift+F waits: the top of the stack `hits` on `document` is the template.
/// Returns false when no pick was waiting, so the click picks as usual.
pub fn take_pick(shell: &mut Shell, document: Uuid, hits: &[Uuid]) -> bool {
    if !picking(shell) {
        return false;
    }
    if let Some(state) = state_mut(shell) {
        state.picking = false;
    }
    let Some(hit) = hits.first() else {
        return true;
    };
    use reda_engine::render::Child;
    let template = match crate::selection::rules::child(shell, document, *hit) {
        Some(Child::Pad { footprint, pad }) => Template::Pad(PadRef {
            footprint: *footprint,
            pad: *pad,
        }),
        // Other children of a Component or Footprint can't be found: their owner is.
        Some(child) => Template::Object(child.owner()),
        None => Template::Object(*hit),
    };
    if let Err(why) = open(shell, document, template) {
        shell.report(why);
    }
    true
}

/// Opens the window on `template`, on `document`'s canvas. Refused for a kind the
/// Selection Filter leaves out.
pub fn open(shell: &mut Shell, document: Uuid, template: Template) -> Result<(), String> {
    let allowed = match template {
        Template::Object(o) => crate::selection::rules::allowed(shell, document, o),
        Template::Pad(_) => crate::selection::rules::kind_allowed(shell, document, FilterKind::Pad),
    };
    if !allowed {
        return Err(tr!("similar-filtered"));
    }
    let project = shell
        .engine()
        .project()
        .ok_or_else(|| tr!("similar-needs-object"))?;
    let info = describe_template(project, template).map_err(|e| e.message)?;
    let title = match template {
        Template::Object(o) => match project.object(o) {
            Some(Object::Unit(u)) => tr!("similar-title", designator = u.object.designator.clone()),
            Some(Object::FootprintInstance(f)) => {
                tr!("similar-title", designator = f.object.designator.clone())
            }
            _ => tr!("similar-title-kind", kind = info.kind.clone()),
        },
        Template::Pad(_) => tr!("similar-title-kind", kind = info.kind.clone()),
    };
    let rows = info
        .rows
        .into_iter()
        .map(|r| Row {
            value: match &r.value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            rule: if r.group == AttributeGroup::Kind {
                Rule::Same
            } else {
                Rule::Any
            },
            name: r.name,
            group: r.group,
        })
        .collect();
    let window = SimilarWindow {
        document,
        template,
        title,
        rows,
        scope: info.scopes[0],
        scopes: info.scopes,
        select: true,
        zoom: false,
        clear_existing: true,
        show: Show::Mask,
        open_properties: false,
        found: None,
        error: None,
    };
    if let Some(state) = state_mut(shell) {
        state.window = Some(window);
    }
    Ok(())
}

/// Apply: finds the matches and shows them as the options say, keeping the window open.
/// Returns how many it found.
pub fn apply(shell: &mut Shell) -> Result<usize, CommandError> {
    let Some(w) = window(shell).cloned() else {
        return Err(CommandError::unavailable(
            "the Select Similar window is not open",
        ));
    };
    let out = match shell.run("objects.find_similar", w.params()) {
        Ok(out) => out,
        Err(e) => {
            if let Some(w) = window_mut(shell) {
                w.error = Some(e.message.clone());
            }
            return Err(e);
        }
    };
    let ids = |key: &str| -> Vec<Value> { out[key].as_array().cloned().unwrap_or_default() };
    let project = shell.engine().project();
    // The Selection Filter is honoured: filtered kinds are left out.
    let objects: Vec<Uuid> = ids("objects")
        .iter()
        .filter_map(|v| v.as_str()?.parse().ok())
        .filter(|o: &Uuid| {
            let home = project
                .and_then(|p| p.object(*o))
                .and_then(|o| match o.scope() {
                    Scope::Document(d) => Some(d),
                    _ => None,
                })
                .unwrap_or(w.document);
            crate::selection::rules::allowed(shell, home, *o)
        })
        .collect();
    let pads: Vec<PadRef> =
        if crate::selection::rules::kind_allowed(shell, w.document, FilterKind::Pad) {
            ids("pads")
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect()
        } else {
            Vec::new()
        };
    let count = objects.len() + pads.len();
    let mut shown: Vec<Uuid> = objects.clone();
    shown.extend(pads.iter().map(|p| p.footprint));
    shown.dedup();
    if w.select {
        let mut selection = if w.clear_existing {
            crate::selection::Selection::default()
        } else {
            shell.selection().clone()
        };
        for o in &objects {
            if !selection.objects.contains(o) {
                selection.objects.push(*o);
            }
        }
        for p in &pads {
            if !selection.pads.contains(p) {
                selection.pads.push(*p);
            }
        }
        shell.run(
            "selection.set",
            json!({"objects": selection.objects, "pads": selection.pads,
                   "fields": selection.fields, "texts": selection.texts}),
        )?;
    }
    match w.show {
        Show::Normal => shell.run("view.highlight", json!({}))?,
        Show::Dim => shell.run("view.highlight", json!({ "objects": shown }))?,
        Show::Mask => shell.run("view.highlight", json!({ "objects": shown, "mask": true }))?,
    };
    if w.zoom {
        let closeness = shell.preferences().editing.highlight.closeness;
        shell.zoom_closely(w.document, &shown, closeness);
    }
    if w.open_properties {
        shell.run("panel.show:properties", json!({}))?;
    }
    if let Some(w) = window_mut(shell) {
        w.found = Some(count);
        w.error = None;
    }
    Ok(count)
}

/// Draws the window while it is open: rows by group, the scope, the options and the
/// buttons. It doesn't block the canvas.
pub fn show(ctx: &egui::Context, shell: &mut Shell) {
    let Some(mut w) = window(shell).cloned() else {
        return;
    };
    let mut open = true;
    let mut action = None;
    egui::Window::new(w.title.clone())
        .id(Id::NULL.with("select-similar-window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(420.0)
        .show(ctx, |ui| {
            let max = (ctx.content_rect().height() * 0.5).max(160.0);
            egui::ScrollArea::vertical().max_height(max).show(ui, |ui| {
                for (group, heading) in [
                    (AttributeGroup::Kind, tr!("similar-group-kind")),
                    (AttributeGroup::ObjectSpecific, tr!("similar-group-object")),
                    (AttributeGroup::Graphical, tr!("similar-group-graphical")),
                ] {
                    if !w.rows.iter().any(|r| r.group == group) {
                        continue;
                    }
                    ui.strong(heading);
                    egui::Grid::new(("similar-rows", group as u8))
                        .num_columns(3)
                        .show(ui, |ui| {
                            for row in w.rows.iter_mut().filter(|r| r.group == group) {
                                rule_row(ui, row);
                                ui.end_row();
                            }
                        });
                    ui.add_space(4.0);
                }
            });
            ui.add_space(6.0);
            if w.scopes.len() > 1 {
                ui.horizontal(|ui| {
                    for scope in w.scopes.clone() {
                        let label = match scope {
                            SimilarScope::Sheet => tr!("similar-scope-sheet"),
                            SimilarScope::Project => tr!("similar-scope-project"),
                            SimilarScope::Board => tr!("similar-scope-board"),
                            SimilarScope::Element => tr!("similar-scope-element"),
                        };
                        ui.radio_value(&mut w.scope, scope, label);
                    }
                });
            }
            ui.checkbox(&mut w.select, tr!("similar-option-select"));
            ui.checkbox(&mut w.zoom, tr!("similar-option-zoom"));
            ui.checkbox(&mut w.clear_existing, tr!("similar-option-clear"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut w.show, Show::Normal, tr!("similar-option-normal"));
                ui.radio_value(&mut w.show, Show::Dim, tr!("similar-option-dim"));
                ui.radio_value(&mut w.show, Show::Mask, tr!("similar-option-mask"));
            });
            ui.checkbox(&mut w.open_properties, tr!("similar-option-properties"));
            if let Some(error) = &w.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            } else if let Some(found) = w.found {
                ui.label(tr!("similar-found", count = found as i64));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("similar-apply")).clicked() {
                    action = Some(Button::Apply);
                }
                if ui.button(tr!("similar-ok")).clicked() {
                    action = Some(Button::Ok);
                }
                if ui.button(tr!("dialog-cancel")).clicked() {
                    action = Some(Button::Cancel);
                }
            });
        });
    if let Some(window) = window_mut(shell) {
        // Rows and options as edited this frame; the last result stays.
        let (found, error) = (window.found, window.error.clone());
        *window = w;
        window.found = found;
        window.error = error;
    }
    match action {
        Some(Button::Cancel) => close(shell),
        Some(Button::Apply) => {
            let _ = apply(shell);
        }
        Some(Button::Ok) if apply(shell).is_ok() => close(shell),
        Some(Button::Ok) | None => {}
    }
    if !open {
        close(shell);
    }
}

enum Button {
    Apply,
    Ok,
    Cancel,
}

/// One row: the attribute, the template's value and the rule.
fn rule_row(ui: &mut egui::Ui, row: &mut Row) {
    ui.label(&row.name);
    let value = if row.value.chars().count() > 32 {
        let short: String = row.value.chars().take(31).collect();
        format!("{short}…")
    } else {
        row.value.clone()
    };
    ui.label(value).on_hover_text(&row.value);
    ui.horizontal(|ui| {
        let current = match &row.rule {
            Rule::Any => tr!("similar-rule-any"),
            Rule::Same => tr!("similar-rule-same"),
            Rule::Different => tr!("similar-rule-different"),
            Rule::Pattern(_) => tr!("similar-rule-pattern"),
        };
        egui::ComboBox::from_id_salt(("similar-rule", &row.name))
            .selected_text(current)
            .show_ui(ui, |ui| {
                let pattern = match &row.rule {
                    Rule::Pattern(p) => p.clone(),
                    _ => row.value.clone(),
                };
                for (rule, label) in [
                    (Rule::Any, tr!("similar-rule-any")),
                    (Rule::Same, tr!("similar-rule-same")),
                    (Rule::Different, tr!("similar-rule-different")),
                    (Rule::Pattern(pattern), tr!("similar-rule-pattern")),
                ] {
                    let on = std::mem::discriminant(&row.rule) == std::mem::discriminant(&rule);
                    if ui.selectable_label(on, label).clicked() {
                        row.rule = rule;
                    }
                }
            })
            .response
            .widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::ComboBox, true, row.name.clone())
            });
        if let Rule::Pattern(p) = &mut row.rule {
            let name = row.name.clone();
            ui.add(egui::TextEdit::singleline(p).desired_width(90.0))
                .widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::TextInput, true, name.clone())
                });
        }
    });
}
