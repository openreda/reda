//! Paste Array (library.md#placing-pins-and-pads): copies of the selected Pads of a
//! Footprint, or Pins of a Symbol, `count` times, each `dx`, `dy` further on, with numbers
//! stepped by `number_step` (letters too) and Pin names by `name_step`, optionally without
//! leading zeroes. It runs one `batch` of the add Commands, so it is one undo step.
//!
//! The view Command `library.paste_array` works out that `batch` ([`batch`]); from the
//! window it always opens [`PasteArrayDialog`] first, whose Paste runs the `batch`.

use std::any::Any;

use egui::{Grid, Id, Role, WidgetInfo};
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{LibraryElement, Uuid, auto_increment};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::shell::Shell;
use crate::tr;

pub const COMMAND: &str = "library.paste_array";

/// What Paste Array makes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArrayOptions {
    /// How many copies of the selection.
    pub count: u32,
    /// How far each copy's numbers step from the one before; letters step too (A → B).
    #[serde(default = "one")]
    pub number_step: i64,
    /// How far each copy's Pin names step (their trailing number).
    #[serde(default = "one")]
    pub name_step: i64,
    /// How far each copy is from the one before.
    pub dx: reda_engine::command::Length,
    pub dy: reda_engine::command::Length,
    /// Writes stepped numbers and names without leading zeroes: `09` → `10`, `01` → `2`.
    #[serde(default)]
    pub remove_leading_zeroes: bool,
}

fn one() -> i64 {
    1
}

/// `text` stepped `by` with [`auto_increment`], its trailing digits without leading
/// zeroes when `strip`.
fn stepped(text: &str, by: i64, number: bool, strip: bool) -> String {
    let next = if number {
        auto_increment::step_number(text, by)
    } else {
        auto_increment::step(text, by)
    };
    if !strip {
        return next;
    }
    let digits = next.len() - next.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (head, tail) = next.split_at(next.len() - digits);
    let tail = tail.trim_start_matches('0');
    let tail = if tail.is_empty() && digits > 0 {
        "0"
    } else {
        tail
    };
    format!("{head}{tail}")
}

/// The `batch` that pastes the array of `objects` (Pads of a Footprint, Pins of a Symbol)
/// into `element`. `Err` says why there is nothing to paste.
pub fn batch(
    element: &LibraryElement,
    objects: &[Uuid],
    o: &ArrayOptions,
) -> Result<Value, String> {
    let (dx, dy) = (o.dx.nm(), o.dy.nm());
    let offset = |k: i64| Point::new(Nm(dx.0 * k), Nm(dy.0 * k));
    let strip = o.remove_leading_zeroes;
    let mut steps = Vec::new();
    match element {
        LibraryElement::Footprint(fp) => {
            let pads: Vec<_> = fp
                .pads
                .iter()
                .filter(|p| objects.contains(&p.uuid))
                .collect();
            for k in 1..=i64::from(o.count) {
                for pad in &pads {
                    let mut params = serde_json::to_value(pad).map_err(|e| e.to_string())?;
                    let map = params.as_object_mut().expect("a Pad is an object");
                    map.remove("uuid");
                    let at = pad.at + offset(k);
                    map.insert("footprint".into(), json!(fp.uuid));
                    map.insert(
                        "number".into(),
                        json!(stepped(&pad.number, k * o.number_step, true, strip)),
                    );
                    map.insert("at".into(), json!({"x": at.x.0, "y": at.y.0}));
                    steps.push(json!({"id": "library.footprint.pad.add", "params": params}));
                }
            }
        }
        LibraryElement::Symbol(s) => {
            let pins: Vec<_> = s
                .pins
                .iter()
                .filter(|p| objects.contains(&p.uuid))
                .collect();
            for k in 1..=i64::from(o.count) {
                for pin in &pins {
                    let mut params = serde_json::to_value(pin).map_err(|e| e.to_string())?;
                    let map = params.as_object_mut().expect("a Pin is an object");
                    map.remove("uuid");
                    let at = pin.at + offset(k);
                    map.insert("symbol".into(), json!(s.uuid));
                    map.insert(
                        "number".into(),
                        json!(stepped(&pin.number, k * o.number_step, true, strip)),
                    );
                    map.insert(
                        "name".into(),
                        json!(stepped(&pin.name, k * o.name_step, false, strip)),
                    );
                    map.insert("at".into(), json!({"x": at.x.0, "y": at.y.0}));
                    steps.push(json!({"id": "library.symbol.pin.add", "params": params}));
                }
            }
        }
        LibraryElement::Part(_) => return Err(tr!("paste-array-not-here")),
    }
    if steps.is_empty() {
        return Err(tr!("paste-array-nothing"));
    }
    Ok(json!({ "commands": steps }))
}

/// Adds `library.paste_array` to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(PasteArray);
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteArrayParams {
    /// The open Footprint or Symbol.
    pub element: Uuid,
    /// The Pads or Pins to repeat.
    pub objects: Vec<Uuid>,
    #[serde(flatten)]
    pub options: ArrayOptions,
}

struct PasteArray;

impl Command for PasteArray {
    const META: Meta = Meta {
        id: COMMAND,
        title: "Paste Array",
        description: "Works out the `batch` of `library.footprint.pad.add` or \
                      `library.symbol.pin.add` Commands that repeats the given Pads or Pins \
                      of an open Footprint or Symbol `count` times, each `dx`, `dy` further \
                      on, numbers stepped by `number_step` and Pin names by `name_step`. \
                      Running that `batch` is one undo step; the window's Paste Array \
                      dialog does both.",
        category: "library",
        icon: "paste",
        kind: Kind::View,
    };
    type Params = PasteArrayParams;
    type Output = Value;

    fn run(&self, cx: &mut Context<'_>, p: PasteArrayParams) -> Result<Value, CommandError> {
        let element = cx
            .project()?
            .element(p.element)
            .cloned()
            .ok_or_else(|| CommandError::not_found(format!("no open element {}", p.element)))?;
        let batch = batch(&element, &p.objects, &p.options)
            .map_err(|why| CommandError::invalid_params("/objects", why))?;
        Ok(json!({ "batch": batch }))
    }
}

/// The modal Paste Array dialog over the selection of one open element.
pub struct PasteArrayDialog {
    element: Option<LibraryElement>,
    objects: Vec<Uuid>,
    pub count: String,
    pub number_step: String,
    pub name_step: String,
    pub dx: String,
    pub dy: String,
    pub remove_leading_zeroes: bool,
    error: Option<String>,
}

impl PasteArrayDialog {
    /// The dialog for `params.element` (else the active Document) and `params.objects`
    /// (else the selection on it).
    pub fn new(shell: &Shell, params: &Value) -> PasteArrayDialog {
        let element_id: Option<Uuid> = params["element"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .or_else(|| shell.layout().active_document());
        let element = element_id.and_then(|e| shell.engine().element(e).cloned());
        let objects = match params["objects"].as_array() {
            Some(list) => list
                .iter()
                .filter_map(|o| o.as_str()?.parse().ok())
                .collect(),
            None => {
                element_id.map_or_else(Vec::new, |e| crate::element_editor::selected_on(shell, e))
            }
        };
        let pitch = match element {
            Some(LibraryElement::Symbol(_)) => "2.54mm",
            _ => "1.27mm",
        };
        PasteArrayDialog {
            element,
            objects,
            count: "4".into(),
            number_step: "1".into(),
            name_step: "1".into(),
            dx: "0mm".into(),
            dy: format!("-{pitch}"),
            remove_leading_zeroes: false,
            error: None,
        }
    }

    /// The options typed, when they read.
    pub fn options(&self) -> Result<ArrayOptions, String> {
        let int = |t: &str| {
            t.trim()
                .parse::<i64>()
                .map_err(|_| tr!("paste-array-bad-number"))
        };
        let length = |t: &str| {
            crate::pad_form::draw::read_length(t, reda_engine::preferences::Units::Mm)
                .map(reda_engine::command::Length)
                .ok_or_else(|| tr!("paste-array-bad-length"))
        };
        let count = int(&self.count)?;
        if !(1..=1000).contains(&count) {
            return Err(tr!("paste-array-bad-count"));
        }
        Ok(ArrayOptions {
            count: count as u32,
            number_step: int(&self.number_step)?,
            name_step: int(&self.name_step)?,
            dx: length(&self.dx)?,
            dy: length(&self.dy)?,
            remove_leading_zeroes: self.remove_leading_zeroes,
        })
    }

    /// The `batch` params Paste runs.
    pub fn params(&self) -> Result<Value, String> {
        let element = self
            .element
            .as_ref()
            .ok_or_else(|| tr!("paste-array-not-here"))?;
        batch(element, &self.objects, &self.options()?)
    }
}

impl ParamsDialog for PasteArrayDialog {
    fn command(&self) -> &str {
        "batch"
    }

    fn show(&mut self, ctx: &egui::Context, _shell: &mut Shell) -> Outcome {
        let mut outcome = Outcome::Open;
        let id = Id::NULL.with("paste-array");
        let symbol = matches!(self.element, Some(LibraryElement::Symbol(_)));
        let modal = crate::modal::Dialog::new(id, tr!("paste-array-title"))
            .width(360.0)
            .show(ctx, |ui| {
                ui.label(tr!("paste-array-intro", count = self.objects.len()));
                ui.add_space(6.0);
                Grid::new(id.with("grid")).num_columns(2).show(ui, |ui| {
                    let field = |ui: &mut egui::Ui, label: String, text: &mut String| {
                        ui.label(&label);
                        ui.add(egui::TextEdit::singleline(text).desired_width(120.0))
                            .widget_info(|| {
                                WidgetInfo::labeled(Role::TextInput, true, label.clone())
                            });
                        ui.end_row();
                    };
                    field(ui, tr!("paste-array-count"), &mut self.count);
                    field(ui, tr!("paste-array-number-step"), &mut self.number_step);
                    if symbol {
                        field(ui, tr!("paste-array-name-step"), &mut self.name_step);
                    }
                    field(ui, tr!("paste-array-dx"), &mut self.dx);
                    field(ui, tr!("paste-array-dy"), &mut self.dy);
                });
                ui.checkbox(
                    &mut self.remove_leading_zeroes,
                    tr!("paste-array-remove-leading-zeroes"),
                );
                let params = self.params();
                if let Err(why) = &params {
                    ui.colored_label(ui.visuals().warn_fg_color, why);
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let paste =
                        ui.add_enabled(params.is_ok(), egui::Button::new(tr!("paste-array-run")));
                    if paste.clicked()
                        && let Ok(params) = params
                    {
                        outcome = Outcome::Run(params);
                    }
                    if ui.button(tr!("dialog-cancel")).clicked() {
                        outcome = Outcome::Cancel;
                    }
                });
            });
        if modal.should_close() && outcome == Outcome::Open {
            outcome = Outcome::Cancel;
        }
        outcome
    }

    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
