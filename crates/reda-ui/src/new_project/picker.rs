//! The Board Template picker of New Project and Add PCB Document: the searchable list
//! grouped by category, and for the picked template its description, the live preview,
//! the parameters and the Includes ticks (docs/spec/v0.2.5/creation-dialog.md).
//!
//! The list is `library.board_template.list`'s ([`list`]) and the preview is the frame
//! `library.board_template.preview` returns ([`evaluate_choice`]), both over the
//! Libraries scanned when the dialog opened. It builds the `board_template` param of the
//! one `project.new` or `document.add` the dialog runs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use egui::{Role, Ui, WidgetInfo, vec2};
use reda_engine::commands::board_template::{
    ListedParameter, ListedTemplate, ParameterKind, evaluate_choice, list,
};
use reda_engine::library::LibraryIndex;
use reda_engine::library::LibraryOrigin;
use reda_engine::library::board_template::{
    BoardTemplateChoice, Combination, Frame, ResolvedValue, TemplateValue, gate_holds,
};
use reda_engine::model::Side;
use reda_engine::model::Uuid;
use reda_engine::model::geom::Nm;
use serde_json::{Value, json};

use super::preview::{self, Look, trim};
use crate::settings::{self, layout};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// The category the picker shows first.
const GENERIC: &str = "Generic";
/// The list pane's width.
const LIST_WIDTH: f32 = 250.0;
/// The list's height.
const LIST_HEIGHT: f32 = 520.0;
/// The preview's size.
const PREVIEW: [f32; 2] = [660.0, 250.0];
/// The window height kept for everything but the list, and the least list height.
const LIST_ROOM: f32 = 330.0;
const MIN_LIST: f32 = 200.0;
/// The window height kept for everything but the preview, and its least height.
const PREVIEW_ROOM: f32 = 640.0;
const MIN_PREVIEW: f32 = 150.0;
/// The lines a template's description takes at most; all of it shows on hover.
const DESCRIPTION_ROWS: usize = 3;
/// The parameters column's width.
const PARAMS_WIDTH: f32 = 330.0;
/// The Includes list's greatest height before it scrolls.
const INCLUDES_HEIGHT: f32 = 190.0;
/// How wide a length field is.
const LENGTH_WIDTH: f32 = 90.0;
/// A choice with more options than this is a dropdown, not a segmented control.
const SEGMENTED_MAX: usize = 3;

/// What is wrong with one typed parameter value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputProblem {
    pub parameter: String,
    pub message: String,
}

/// Why the dialog can't send its `board_template` yet: a typed value is bad, or the
/// frame fails to evaluate or is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotReady;

/// Includes rows with their labels.
type Rows = Vec<(Uuid, String)>;

/// The picker's state.
#[derive(Clone)]
pub struct Picker {
    index: Arc<LibraryIndex>,
    templates: Vec<ListedTemplate>,
    /// The picked template; `None` is "No outline (draw it later)".
    pub selected: Option<Uuid>,
    pub search: String,
    /// Each template's parameter values as typed: an option key, or a length in mm.
    texts: BTreeMap<Uuid, BTreeMap<String, String>>,
    /// Each template's unticked Includes: connector, hole and drawing-group rows.
    unticked: BTreeMap<Uuid, BTreeSet<Uuid>>,
    /// The Includes groups shown expanded, by their first row.
    expanded: BTreeSet<Uuid>,
    /// The last choice evaluated and what it gave.
    evaluated: Option<(BoardTemplateChoice, Result<Frame, String>)>,
    /// Each template's last frame that evaluated, drawn while a typed value is bad.
    last: BTreeMap<Uuid, Frame>,
}

impl std::fmt::Debug for Picker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picker")
            .field("selected", &self.selected)
            .field("search", &self.search)
            .finish_non_exhaustive()
    }
}

impl Picker {
    /// The picker over the templates `index` holds. New Project (`pcb` false) leaves out
    /// the Project's Libraries; Add PCB Document lists every origin.
    pub fn new(index: Arc<LibraryIndex>, pcb: bool) -> Picker {
        let mut templates: Vec<_> = list(&index, None)
            .templates
            .into_iter()
            .filter(|t| pcb || t.origin != LibraryOrigin::Project)
            .collect();
        templates.sort_by(|a, b| {
            let key = |t: &ListedTemplate| {
                (
                    category(t) != GENERIC,
                    category(t).to_owned(),
                    t.name.clone(),
                )
            };
            key(a).cmp(&key(b))
        });
        let texts = templates
            .iter()
            .map(|t| (t.element, default_texts(t)))
            .collect();
        Picker {
            index,
            templates,
            selected: None,
            search: String::new(),
            texts,
            unticked: BTreeMap::new(),
            expanded: BTreeSet::new(),
            evaluated: None,
            last: BTreeMap::new(),
        }
    }

    /// Every template the list offers, in list order.
    pub fn templates(&self) -> &[ListedTemplate] {
        &self.templates
    }

    /// The picked template.
    pub fn template(&self) -> Option<&ListedTemplate> {
        let uuid = self.selected?;
        self.templates.iter().find(|t| t.element == uuid)
    }

    /// Picks the template named `name`, or "No outline" for `None`.
    pub fn pick(&mut self, name: Option<&str>) {
        self.selected = name.and_then(|n| {
            self.templates
                .iter()
                .find(|t| t.name == n)
                .map(|t| t.element)
        });
    }

    /// Sets a parameter of the picked template as typed: an option key, or mm.
    pub fn set_value(&mut self, name: &str, text: &str) {
        if let Some(uuid) = self.selected {
            let texts = self.texts.entry(uuid).or_default();
            texts.insert(name.to_owned(), text.to_owned());
        }
    }

    /// Ticks or unticks an Includes row of the picked template.
    pub fn set_ticked(&mut self, row: Uuid, on: bool) {
        if let Some(uuid) = self.selected {
            let unticked = self.unticked.entry(uuid).or_default();
            if on {
                unticked.remove(&row);
            } else {
                unticked.insert(row);
            }
        }
    }

    fn texts(&self, t: &ListedTemplate) -> BTreeMap<String, String> {
        self.texts.get(&t.element).cloned().unwrap_or_default()
    }

    /// The parameters the dialog shows for `t` now: those whose `when` holds.
    pub fn visible<'a>(&self, t: &'a ListedTemplate) -> Vec<&'a ListedParameter> {
        let settled = settled(t, &self.texts(t));
        (t.parameters.iter())
            .filter(|p| gate_holds(p.when.as_ref(), &settled))
            .collect()
    }

    /// The `values` of the picked template's choice: every visible parameter, or the
    /// first one whose typed value is bad.
    fn values(&self, t: &ListedTemplate) -> Result<BTreeMap<String, TemplateValue>, InputProblem> {
        let texts = self.texts(t);
        let settled = settled(t, &texts);
        let mut values = BTreeMap::new();
        for p in self.visible(t) {
            let text = texts.get(&p.name).cloned().unwrap_or_default();
            let value = match p.kind {
                ParameterKind::Choice => TemplateValue::Text(text),
                ParameterKind::Length => {
                    let nm = length_value(p, &settled, &text).map_err(|message| InputProblem {
                        parameter: p.name.clone(),
                        message,
                    })?;
                    TemplateValue::Text(format!("{}mm", trim(mm(nm))))
                }
            };
            values.insert(p.name.clone(), value);
        }
        Ok(values)
    }

    /// The picked template's choice as the dialog would send it: `None` for "No
    /// outline", else the choice or the first bad typed value.
    pub fn choice(&self) -> Option<Result<BoardTemplateChoice, InputProblem>> {
        let t = self.template()?;
        Some(self.values(t).map(|values| BoardTemplateChoice {
            element: t.element,
            values,
            exclude: (self.unticked.get(&t.element).into_iter().flatten().copied()).collect(),
        }))
    }

    /// The picked template's frame at its choice, evaluated again only when the choice
    /// changed: the frame, or the evaluation's error. `None` for "No outline" or a bad
    /// typed value.
    pub fn frame(&mut self) -> Option<Result<&Frame, &str>> {
        let choice = self.choice()?.ok()?;
        if self.evaluated.as_ref().is_none_or(|(c, _)| *c != choice) {
            let result = evaluate_choice(&self.index, &choice).map_err(|e| e.message);
            if let Ok(frame) = &result {
                self.last.insert(choice.element, frame.clone());
            }
            self.evaluated = Some((choice, result));
        }
        let (_, result) = self.evaluated.as_ref()?;
        Some(result.as_ref().map_err(String::as_str))
    }

    /// The `board_template` param: `Ok(None)` for "No outline", `Err` while a value is
    /// bad, the frame fails to evaluate or it is invalid.
    pub fn board_template(&mut self) -> Result<Option<Value>, NotReady> {
        let Some(choice) = self.choice() else {
            return Ok(None);
        };
        let choice = choice.map_err(|_| NotReady)?;
        match self.frame() {
            Some(Ok(frame)) if frame.is_valid() => {}
            _ => return Err(NotReady),
        }
        let values: serde_json::Map<String, Value> = (choice.values.iter())
            .map(|(k, v)| {
                let v = match v {
                    TemplateValue::Text(s) => json!(s),
                    TemplateValue::Number(n) => json!(n),
                };
                (k.clone(), v)
            })
            .collect();
        let mut param = json!({"element": choice.element, "values": values});
        if !choice.exclude.is_empty() {
            param["exclude"] = json!(choice.exclude);
        }
        Ok(Some(param))
    }

    /// The copper Layer count the picked template suggests at its values.
    pub fn suggested_layers(&mut self) -> Option<u8> {
        let fallback = self.template()?.suggested_layers;
        Some(match self.frame() {
            Some(Ok(frame)) => frame.suggested_layers,
            _ => fallback,
        })
    }

    /// Draws the list on the left and the picked template on the right. `settings` draws
    /// the Fab Preset, Copper Layers and Construction under the parameters.
    pub fn show(&mut self, ui: &mut Ui, look: &Look, settings: impl FnOnce(&mut Ui)) {
        let t = ui.tokens();
        // The dialog keeps to the window: the list and the preview give up height.
        let screen = ui.ctx().content_rect().height();
        let list =
            (screen - t.size.of(LIST_ROOM)).clamp(t.size.of(MIN_LIST), t.size.of(LIST_HEIGHT));
        let preview =
            (screen - t.size.of(PREVIEW_ROOM)).clamp(t.size.of(MIN_PREVIEW), t.size.of(PREVIEW[1]));
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(t.size.of(LIST_WIDTH));
                self.list(ui, list);
            });
            ui.add_space(t.space.section);
            ui.vertical(|ui| self.detail(ui, look, preview, settings));
        });
    }

    fn list(&mut self, ui: &mut Ui, height: f32) {
        let t = ui.tokens();
        let search = tr!("new-project-search");
        let field = ui.add(
            widgets::text_field(&mut self.search)
                .search()
                .hint(&search)
                .width(t.size.of(LIST_WIDTH)),
        );
        field.widget_info(|| WidgetInfo::labeled(Role::SearchInput, true, &search));
        let query = self.search.trim().to_lowercase();
        let matches = |t: &ListedTemplate| {
            query.is_empty()
                || [t.name.as_str(), t.description.as_str(), category(t)]
                    .iter()
                    .any(|s| s.to_lowercase().contains(&query))
        };
        let none = tr!("new-project-no-outline");
        let none_matches = query.is_empty() || none.to_lowercase().contains(&query);
        egui::ScrollArea::vertical()
            .id_salt("new-project-templates")
            .auto_shrink([true, false])
            .max_height(height)
            .show(ui, |ui| {
                let mut categories: Vec<&str> = Vec::new();
                for t in self.templates.iter().filter(|t| matches(t)) {
                    if !categories.contains(&category(t)) {
                        categories.push(category(t));
                    }
                }
                if none_matches && !categories.contains(&GENERIC) {
                    categories.insert(0, GENERIC);
                }
                let mut picked = None;
                for name in categories {
                    ui.add_space(t.space.stack);
                    layout::muted(ui, category_label(name));
                    if name == GENERIC && none_matches {
                        let row = widgets::list_row(&none)
                            .strong(true)
                            .selected(self.selected.is_none())
                            .show(ui);
                        if row.response.clicked() {
                            picked = Some(None);
                        }
                    }
                    for tpl in self
                        .templates
                        .iter()
                        .filter(|t| matches(t) && category(t) == name)
                    {
                        let row = widgets::list_row(&tpl.name)
                            .selected(self.selected == Some(tpl.element))
                            .show(ui);
                        if row.response.clicked() {
                            picked = Some(Some(tpl.element));
                        }
                    }
                }
                if let Some(picked) = picked {
                    self.selected = picked;
                }
            });
    }

    fn detail(&mut self, ui: &mut Ui, look: &Look, height: f32, settings: impl FnOnce(&mut Ui)) {
        let t = ui.tokens();
        let size = vec2(t.size.of(PREVIEW[0]), height);
        let Some(template) = self.template().cloned() else {
            settings::strong(ui, tr!("new-project-no-outline"));
            layout::muted(ui, tr!("new-project-no-outline-note"));
            preview::show(ui, None, size, look, &tr!("new-project-preview-empty"));
            ui.add_space(t.space.section);
            ui.vertical(|ui| {
                ui.set_width(t.size.of(PARAMS_WIDTH));
                settings(ui);
            });
            return;
        };
        settings::strong(ui, &template.name);
        let mut job = t.ty.body.job(&template.description, t.color.text.muted);
        job.wrap.max_width = size.x;
        job.wrap.max_rows = DESCRIPTION_ROWS;
        ui.add(egui::Label::new(job)).tooltip(&template.description);
        let input = self.choice().and_then(Result::err);
        let evaluated = self.frame().map(|r| r.cloned().map_err(str::to_owned));
        let shown = match &evaluated {
            Some(Ok(frame)) => Some(frame.clone()),
            _ => self.last.get(&template.element).cloned(),
        };
        preview::show(
            ui,
            shown.as_ref(),
            size,
            look,
            &tr!("new-project-preview-none"),
        );
        preview::legend(ui, look);
        if let Some(problem) = &input {
            layout::error(ui, &problem.message);
        }
        match &evaluated {
            Some(Err(message)) => {
                layout::error(ui, message);
            }
            Some(Ok(frame)) if !frame.is_valid() => {
                layout::error(ui, tr!("new-project-frame-invalid"));
                for problem in &frame.problems {
                    layout::error(ui, &problem.message);
                }
            }
            _ => {}
        }
        ui.add_space(t.space.section);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(t.size.of(PARAMS_WIDTH));
                if !template.parameters.is_empty() {
                    settings::strong(ui, tr!("new-project-parameters"));
                    self.parameters(ui, &template, input.as_ref());
                    ui.add_space(t.space.section);
                }
                settings(ui);
            });
            ui.add_space(t.space.section);
            ui.vertical(|ui| {
                if let Some(frame) = &shown {
                    self.includes(ui, &template, frame);
                }
            });
        });
    }

    fn parameters(&mut self, ui: &mut Ui, template: &ListedTemplate, input: Option<&InputProblem>) {
        let t = ui.tokens();
        let visible: Vec<ListedParameter> = self.visible(template).into_iter().cloned().collect();
        let settled = settled(template, &self.texts(template));
        let texts = self.texts.entry(template.element).or_default();
        widgets::form(("new-project-parameters", template.element)).show(ui, |ui| {
            for p in &visible {
                let label = ui.add(widgets::form_name(&p.label));
                let value = texts.entry(p.name.clone()).or_default();
                match p.kind {
                    ParameterKind::Choice => {
                        let options: Vec<(String, String)> = (p.options.iter().flatten())
                            .map(|o| (o.key.clone(), o.label.clone()))
                            .collect();
                        if options.len() <= SEGMENTED_MAX {
                            ui.add(widgets::segmented(value, &options))
                                .labelled_by(label.id);
                        } else {
                            settings::dropdown(
                                ui,
                                ("new-project-param", template.element, p.name.as_str()),
                                &p.label,
                                value,
                                &options,
                                200.0,
                            );
                        }
                    }
                    ParameterKind::Length => {
                        let bad = input.is_some_and(|i| i.parameter == p.name);
                        ui.horizontal(|ui| {
                            let field = ui.add(
                                widgets::number_field(value)
                                    .unit("mm")
                                    .invalid(bad)
                                    .width(t.size.of(LENGTH_WIDTH)),
                            );
                            let name = p.label.clone();
                            field.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, &name));
                            if let Some((min, max)) = length_range(p, &settled) {
                                layout::muted(
                                    ui,
                                    tr!(
                                        "new-project-length-range",
                                        min = trim(mm(min)),
                                        max = trim(mm(max))
                                    ),
                                );
                            }
                        });
                    }
                }
                ui.end_row();
            }
        });
    }

    fn includes(&mut self, ui: &mut Ui, template: &ListedTemplate, frame: &Frame) {
        let groups = include_groups(template, frame);
        if groups.is_empty() {
            return;
        }
        let t = ui.tokens();
        settings::strong(ui, tr!("new-project-includes"));
        let unticked = self.unticked.entry(template.element).or_default();
        egui::ScrollArea::vertical()
            .id_salt(("new-project-includes", template.element))
            .max_height(t.size.of(INCLUDES_HEIGHT))
            .show(ui, |ui| {
                for group in &groups {
                    let ticked = group
                        .rows
                        .iter()
                        .filter(|(r, _)| !unticked.contains(r))
                        .count();
                    let mut on = ticked == group.rows.len();
                    let mixed = ticked > 0 && !on;
                    let first = group.rows[0].0;
                    let open = self.expanded.contains(&first);
                    ui.horizontal(|ui| {
                        let check =
                            ui.add(widgets::checkbox(&mut on, &group.label).indeterminate(mixed));
                        let label = group.label.clone();
                        check
                            .widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, &label));
                        if check.clicked() {
                            for (row, _) in &group.rows {
                                if on {
                                    unticked.remove(row);
                                } else {
                                    unticked.insert(*row);
                                }
                            }
                        }
                        if group.rows.len() > 1 {
                            let name =
                                tr!("new-project-include-each", group = group.label.as_str());
                            let header =
                                ui.add(widgets::section_header(name, open).width(t.row_height()));
                            if header.clicked() {
                                if open {
                                    self.expanded.remove(&first);
                                } else {
                                    self.expanded.insert(first);
                                }
                            }
                        }
                    });
                    if open && group.rows.len() > 1 {
                        ui.indent(("new-project-include", first), |ui| {
                            for (row, label) in &group.rows {
                                let mut on = !unticked.contains(row);
                                let check = ui.add(widgets::checkbox(&mut on, label));
                                let name = label.clone();
                                check.widget_info(|| {
                                    WidgetInfo::selected(Role::CheckBox, true, on, &name)
                                });
                                if check.clicked() {
                                    if on {
                                        unticked.remove(row);
                                    } else {
                                        unticked.insert(*row);
                                    }
                                }
                            }
                        });
                    }
                }
            });
        if !frame.connectors.is_empty() {
            layout::muted(ui, tr!("new-project-connectors-note"));
        }
    }
}

/// A template's category, "Other" when it has none.
fn category(t: &ListedTemplate) -> &str {
    t.category.as_deref().unwrap_or("Other")
}

fn category_label(name: &str) -> String {
    name.to_uppercase()
}

fn mm(n: Nm) -> f64 {
    n.0 as f64 / 1e6
}

/// Each parameter's default as the dialog shows it: the option key, or the length in mm.
fn default_texts(t: &ListedTemplate) -> BTreeMap<String, String> {
    (t.parameters.iter())
        .map(|p| {
            let text = match &p.default {
                ResolvedValue::Choice(key) => key.clone(),
                ResolvedValue::Length(nm) => trim(mm(*nm)),
            };
            (p.name.clone(), text)
        })
        .collect()
}

/// The choices as evaluation settles them: a choice hidden by its `when` takes its
/// default, which may hide or show another.
fn settled(t: &ListedTemplate, texts: &BTreeMap<String, String>) -> Combination {
    let choices: Vec<&ListedParameter> = (t.parameters.iter())
        .filter(|p| matches!(p.kind, ParameterKind::Choice))
        .collect();
    let default = |p: &ListedParameter| match &p.default {
        ResolvedValue::Choice(key) => key.clone(),
        ResolvedValue::Length(_) => String::new(),
    };
    let mut combination: Combination = choices
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                texts.get(&p.name).cloned().unwrap_or_else(|| default(p)),
            )
        })
        .collect();
    for _ in 0..=choices.len() {
        let mut changed = false;
        for p in &choices {
            if !gate_holds(p.when.as_ref(), &combination) && combination[&p.name] != default(p) {
                combination.insert(p.name.clone(), default(p));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    combination
}

/// The range a length parameter shows beside its field and is checked against, at the
/// `settled` choices: the listed range whose `when` holds, such as PCIe's `card_length`
/// for the lanes chosen. The one place both come from.
pub fn length_range(p: &ListedParameter, settled: &Combination) -> Option<(Nm, Nm)> {
    match (p.ranges.iter().flatten()).find(|r| gate_holds(Some(&r.when), settled)) {
        Some(range) => Some((range.min, range.max)),
        None => Some((p.min?, p.max?)),
    }
}

/// `text` as a value of the length `p`, in mm, checked against its range.
fn length_value(p: &ListedParameter, settled: &Combination, text: &str) -> Result<Nm, String> {
    let text = text.trim();
    let text = text.strip_suffix("mm").unwrap_or(text).trim();
    let value: f64 = text
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite())
        .ok_or_else(|| tr!("new-project-length-bad", name = p.label.as_str()))?;
    let nm = Nm((value * 1e6).round() as i64);
    if let Some((min, max)) = length_range(p, settled)
        && (nm < min || nm > max)
    {
        return Err(tr!(
            "new-project-length-out-of-range",
            name = p.label.as_str(),
            min = trim(mm(min)),
            max = trim(mm(max))
        ));
    }
    Ok(nm)
}

/// One tick row of the Includes: a Part's connectors or holes, or a drawing group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncludeGroup {
    pub label: String,
    /// The rows it ticks, each with its own label for when it is expanded.
    pub rows: Rows,
}

/// The Includes of `frame`: one group per Part (connectors by Part and side, holes by
/// Part, Free Pads together), then one per drawing group. Rows gated out are absent.
pub fn include_groups(template: &ListedTemplate, frame: &Frame) -> Vec<IncludeGroup> {
    let part_name = |row: Uuid| {
        (template.objects.iter())
            .find(|o| o.uuid == row)
            .and_then(|o| o.part_name.clone())
            .unwrap_or_default()
    };
    let protected: BTreeSet<Uuid> = frame.keepouts.iter().filter_map(|k| k.protects).collect();
    let mut groups: Vec<(String, bool, Rows)> = Vec::new();
    let mut add =
        |key: String, row: Uuid, label: String| match groups.iter_mut().find(|g| g.0 == key) {
            Some(g) => {
                g.1 |= protected.contains(&row);
                g.2.push((row, label));
            }
            None => groups.push((key, protected.contains(&row), vec![(row, label)])),
        };
    for (n, c) in frame.connectors.iter().enumerate() {
        let part = part_name(c.uuid);
        let label = match c.side {
            Side::Bottom => tr!(
                "new-project-include-bottom",
                designator = format!("J{}", n + 1),
                part = part.as_str()
            ),
            Side::Top => format!("J{} {part}", n + 1),
        };
        add(format!("connector {} {:?}", c.part, c.side), c.uuid, label);
    }
    let mut mechanical = 0;
    let mut free = 0;
    for h in &frame.holes {
        if h.part.is_some() {
            mechanical += 1;
            let part = part_name(h.uuid);
            add(
                format!("hole {}", part),
                h.uuid,
                format!("H{mechanical} {part}"),
            );
        } else {
            free += 1;
            add(
                "free pad".into(),
                h.uuid,
                tr!("new-project-include-free-pad", n = free),
            );
        }
    }
    let mut out: Vec<IncludeGroup> = groups
        .into_iter()
        .map(|(key, keepout, rows)| {
            let base = if rows.len() == 1 {
                rows[0].1.clone()
            } else {
                let what = if key == "free pad" {
                    tr!("new-project-include-free-pads")
                } else {
                    part_name(rows[0].0)
                };
                format!("{}× {what}", rows.len())
            };
            let label = match (keepout, rows.len()) {
                (false, _) => base,
                (true, 1) => tr!("new-project-include-keepout", what = base.as_str()),
                (true, _) => tr!("new-project-include-keepouts", what = base.as_str()),
            };
            IncludeGroup { label, rows }
        })
        .collect();
    for g in &frame.drawing_groups {
        out.push(IncludeGroup {
            label: g.name.clone(),
            rows: vec![(g.uuid, g.name.clone())],
        });
    }
    out
}
