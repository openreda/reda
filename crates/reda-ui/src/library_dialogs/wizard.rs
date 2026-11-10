//! The Symbol Wizard: a modal dialog over `library.symbol.generate`. It has a layout, a
//! target (this Unit, reopened; a new Unit of this Symbol; or a new Symbol in a Library),
//! the pin grid (number, name, type, side, group) with the column-mapping paste, and a
//! live preview of the layout. Reopening reads the Unit's Pins back as the grid
//! (`wizard::read_unit`); the Command keeps Pin UUIDs by number.
//!
//! Opened from a Part editor (`part` in the call's params), the new Symbol becomes that
//! Part's Symbol once made.

use std::any::Any;

use egui::{Color32, ComboBox, Grid, Id, Pos2, Rect, Role, ScrollArea, Stroke, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::library::wizard::{self, Layout, Side, SymbolPlan, WizardPin};
use reda_engine::model::geom::Nm;
use reda_engine::model::{PinType, Uuid};
use serde_json::{Value, json};

use super::{PasteMapping, PasteOutcome, initial_library};
use crate::command_dialog::{Outcome, ParamsDialog};
use crate::panels::pins::pin_type_name;
use crate::shell::Shell;
use crate::tr;

pub const COMMAND: &str = "library.symbol.generate";

/// What the wizard makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A new Symbol in the Library picked.
    NewSymbol,
    /// A new Unit of the open Symbol.
    NewUnit,
    /// The shown Unit of the open Symbol, laid out again.
    ThisUnit,
}

/// The Symbol Wizard's state.
pub struct SymbolWizardDialog {
    pub layout: Layout,
    pub target: Target,
    /// The open Symbol and its shown Unit it was opened over, if any.
    pub symbol: Option<(Uuid, Uuid)>,
    /// The new Symbol's Library and name.
    pub library: Option<Uuid>,
    pub name: String,
    /// The Pin length, in mil, as typed.
    pub pin_length: String,
    pub rows: Vec<WizardPin>,
    /// The column-mapping paste, while open.
    pub paste: Option<PasteMapping>,
    /// The Part the wizard was opened from, which takes the new Symbol.
    pub part: Option<Uuid>,
    error: Option<String>,
    started: bool,
}

impl SymbolWizardDialog {
    /// The wizard over the active Symbol's shown Unit (reading its Pins back), or for a
    /// new Symbol when no Symbol is active. `given` may name the `part` that takes the
    /// new Symbol.
    pub fn new(shell: &Shell, given: &Value) -> SymbolWizardDialog {
        let part = given["part"].as_str().and_then(|s| s.parse().ok());
        let symbol = crate::element_editor::active_symbol(shell)
            .filter(|_| part.is_none())
            .filter(|s| !crate::element_editor::read_only(shell.engine(), *s))
            .and_then(|s| Some((s, crate::symbol_editor::shown_unit(shell, s)?)));
        let mut dialog = SymbolWizardDialog {
            layout: Layout::Dil,
            target: Target::NewSymbol,
            symbol,
            library: None,
            name: String::new(),
            pin_length: "100".into(),
            rows: Vec::new(),
            paste: None,
            part,
            error: None,
            started: false,
        };
        if let Some((s, unit)) = symbol
            && let Some(sym) = crate::symbol_editor::symbol_of(shell.engine(), s)
        {
            let input = wizard::read_unit(sym, unit);
            dialog.target = Target::ThisUnit;
            if !input.pins.is_empty() {
                dialog.layout = input.layout;
                dialog.pin_length = crate::panels::pins::rows::mil(input.pin_length);
                dialog.rows = input.pins;
            }
        }
        if dialog.rows.is_empty() {
            dialog.rows = (1..=8).map(|n| WizardPin::new(n.to_string(), "")).collect();
        }
        dialog
    }

    /// The Pin length typed, if it reads.
    fn length(&self) -> Option<Nm> {
        crate::panels::pins::rows::parse_length(&self.pin_length).filter(|l| l.0 > 0)
    }

    /// The layout of the rows, or why there is none.
    pub fn plan(&self) -> Result<SymbolPlan, String> {
        let length = self.length().ok_or_else(|| tr!("wizard-bad-length"))?;
        wizard::plan(self.layout, &self.rows, Some(length)).map_err(|e| e.message)
    }

    /// The params to run with, once the grid lays out and the target is complete.
    pub fn run_params(&self) -> Option<Value> {
        self.plan().ok()?;
        let target = match self.target {
            Target::NewSymbol => {
                let name = self.name.trim();
                if name.is_empty() {
                    return None;
                }
                json!({"kind": "new_symbol", "library": self.library?, "name": name})
            }
            Target::NewUnit => json!({"kind": "new_unit", "symbol": self.symbol?.0}),
            Target::ThisUnit => {
                let (symbol, unit) = self.symbol?;
                json!({"kind": "unit", "symbol": symbol, "unit": unit})
            }
        };
        Some(json!({
            "layout": self.layout,
            "pins": self.rows,
            "target": target,
            "pin_length": self.length()?.0,
        }))
    }

    /// Takes the rows the column-mapping paste gives in place of the grid's.
    pub fn take_pasted(&mut self, rows: Vec<WizardPin>) {
        self.rows = rows;
        self.paste = None;
    }

    fn contents(&mut self, ui: &mut Ui, shell: &mut Shell, ready: &Option<Value>) -> Outcome {
        let mut outcome = Outcome::Open;
        let wide = ui.available_width();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width((wide * 0.58).max(360.0));
                self.settings(ui, shell);
                ui.add_space(6.0);
                self.grid(ui);
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.strong(tr!("wizard-preview"));
                preview(ui, self.plan().ok().as_ref());
            });
        });
        if let Err(why) = self.plan() {
            ui.colored_label(ui.visuals().error_fg_color, why);
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let run = ui.add_enabled(ready.is_some(), egui::Button::new(tr!("wizard-run")));
            if run.clicked()
                && let Some(params) = ready
            {
                outcome = Outcome::Run(params.clone());
            }
            if ui.button(tr!("dialog-cancel")).clicked() {
                outcome = Outcome::Cancel;
            }
        });
        outcome
    }

    fn settings(&mut self, ui: &mut Ui, shell: &mut Shell) {
        Grid::new("wizard-settings").num_columns(2).show(ui, |ui| {
            let label = tr!("wizard-layout");
            ui.label(&label);
            ComboBox::from_id_salt("wizard-layout")
                .selected_text(layout_name(self.layout))
                .show_ui(ui, |ui| {
                    for l in LAYOUTS {
                        ui.selectable_value(&mut self.layout, l, layout_name(l));
                    }
                })
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.end_row();
            let label = tr!("wizard-pin-length");
            ui.label(&label);
            let r = ui.add(egui::TextEdit::singleline(&mut self.pin_length).desired_width(60.0));
            r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
            ui.end_row();
            let label = tr!("wizard-target");
            ui.label(&label);
            let mut targets = vec![Target::NewSymbol];
            if self.symbol.is_some() {
                targets = vec![Target::ThisUnit, Target::NewUnit, Target::NewSymbol];
            }
            ComboBox::from_id_salt("wizard-target")
                .selected_text(target_name(self.target))
                .show_ui(ui, |ui| {
                    for t in targets {
                        ui.selectable_value(&mut self.target, t, target_name(t));
                    }
                })
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.end_row();
        });
        if self.target == Target::NewSymbol {
            ui.horizontal(|ui| {
                super::library_picker(
                    ui,
                    shell,
                    Id::NULL.with("wizard-library"),
                    &mut self.library,
                );
            });
            ui.horizontal(|ui| {
                let label = tr!("wizard-name");
                ui.label(&label);
                let r = ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(200.0));
                r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
            });
        }
    }

    fn grid(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong(tr!("wizard-pins"));
            if ui.button(tr!("wizard-add-row")).clicked() {
                let next = self.rows.len() + 1;
                self.rows.push(WizardPin::new(next.to_string(), ""));
            }
            if ui.button(tr!("pin-table-paste")).clicked() {
                self.paste = Some(PasteMapping::default());
            }
        });
        let mut remove = None;
        ScrollArea::vertical()
            .id_salt("wizard-grid")
            .max_height(320.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                Grid::new("wizard-grid-rows")
                    .striped(true)
                    .num_columns(6)
                    .show(ui, |ui| {
                        for key in [
                            "pin-table-number",
                            "pin-table-name",
                            "pin-table-type",
                            "pin-table-side",
                            "pin-table-group",
                        ] {
                            ui.strong(crate::i18n::translate(key, &[]));
                        }
                        ui.label("");
                        ui.end_row();
                        for (i, row) in self.rows.iter_mut().enumerate() {
                            let n = (i + 1).to_string();
                            let cell = |key: &str| {
                                tr!(
                                    "wizard-cell",
                                    row = n.as_str(),
                                    column = crate::i18n::translate(key, &[])
                                )
                            };
                            text(ui, &mut row.number, &cell("pin-table-number"), 48.0);
                            text(ui, &mut row.name, &cell("pin-table-name"), 100.0);
                            let label = cell("pin-table-type");
                            ComboBox::from_id_salt(("wizard-type", i))
                                .selected_text(pin_type_name(row.electrical_type))
                                .show_ui(ui, |ui| {
                                    for t in PinType::ALL {
                                        ui.selectable_value(
                                            &mut row.electrical_type,
                                            *t,
                                            pin_type_name(*t),
                                        );
                                    }
                                })
                                .response
                                .widget_info(|| {
                                    WidgetInfo::labeled(Role::ComboBox, true, label.clone())
                                });
                            let label = cell("pin-table-side");
                            ComboBox::from_id_salt(("wizard-side", i))
                                .selected_text(side_name(row.side))
                                .width(70.0)
                                .show_ui(ui, |ui| {
                                    for s in [
                                        None,
                                        Some(Side::Left),
                                        Some(Side::Right),
                                        Some(Side::Top),
                                        Some(Side::Bottom),
                                    ] {
                                        ui.selectable_value(&mut row.side, s, side_name(s));
                                    }
                                })
                                .response
                                .widget_info(|| {
                                    WidgetInfo::labeled(Role::ComboBox, true, label.clone())
                                });
                            let mut group = row.group.clone().unwrap_or_default();
                            text(ui, &mut group, &cell("pin-table-group"), 48.0);
                            row.group = (!group.trim().is_empty()).then(|| group.trim().to_owned());
                            let what = tr!("wizard-remove-row", row = n.as_str());
                            let b = ui.small_button(tr!("paste-map-remove"));
                            b.widget_info(|| WidgetInfo::labeled(Role::Button, true, what.clone()));
                            if b.clicked() {
                                remove = Some(i);
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some(i) = remove {
            self.rows.remove(i);
        }
    }
}

impl ParamsDialog for SymbolWizardDialog {
    fn command(&self) -> &str {
        COMMAND
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if !self.started {
            self.started = true;
            self.library = initial_library(shell, &Value::Null);
        }
        let ready = self.run_params();
        let mut outcome = Outcome::Open;
        let wide = (ctx.content_rect().width() - 32.0).clamp(560.0, 980.0);
        let modal = crate::modal::Dialog::new(Id::NULL.with("symbol-wizard"), tr!("wizard-title"))
            .width(wide)
            .show(ctx, |ui| {
                outcome = self.contents(ui, shell, &ready);
            });
        if modal.should_close() && outcome == Outcome::Open && self.paste.is_none() {
            outcome = Outcome::Cancel;
        }
        if let Some(paste) = &mut self.paste {
            match paste.show(
                ctx,
                Id::NULL.with("symbol-wizard-paste"),
                &tr!("paste-map-apply"),
            ) {
                PasteOutcome::Open => {}
                PasteOutcome::Cancel => self.paste = None,
                PasteOutcome::Apply(rows) => self.take_pasted(rows),
            }
        }
        outcome
    }

    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }

    fn then(&mut self, result: &Value) -> Option<(String, Value)> {
        let part = self.part?;
        Some((
            "library.part.symbol.set".to_owned(),
            json!({"part": part, "symbol": result["symbol"]}),
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn text(ui: &mut Ui, value: &mut String, label: &str, width: f32) {
    let r = ui.add(egui::TextEdit::singleline(value).desired_width(width));
    let label = label.to_owned();
    r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
}

/// The plan drawn to fit the preview: the body and each Pin's line, with Pin 1 marked.
fn preview(ui: &mut Ui, plan: Option<&SymbolPlan>) {
    let size = egui::vec2(300.0, 300.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let label = tr!("wizard-preview");
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label.clone()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let Some(plan) = plan else {
        return;
    };
    let mut points = vec![plan.body.0, plan.body.1];
    for pin in &plan.pins {
        points.push(pin.at);
    }
    let (min_x, max_x) = (
        points.iter().map(|p| p.x.0).min(),
        points.iter().map(|p| p.x.0).max(),
    );
    let (min_y, max_y) = (
        points.iter().map(|p| p.y.0).min(),
        points.iter().map(|p| p.y.0).max(),
    );
    let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) = (min_x, max_x, min_y, max_y) else {
        return;
    };
    let span = ((max_x - min_x).max(max_y - min_y) as f32).max(1.0);
    let scale = (rect.width() - 24.0) / span;
    let mid = (((min_x + max_x) / 2) as f32, ((min_y + max_y) / 2) as f32);
    let to_screen = |x: i64, y: i64| {
        Pos2::new(
            rect.center().x + (x as f32 - mid.0) * scale,
            rect.center().y - (y as f32 - mid.1) * scale,
        )
    };
    let stroke = Stroke::new(1.5, ui.visuals().text_color());
    let body = Rect::from_two_pos(
        to_screen(plan.body.0.x.0, plan.body.0.y.0),
        to_screen(plan.body.1.x.0, plan.body.1.y.0),
    );
    painter.rect_stroke(body, 0.0, stroke, egui::StrokeKind::Middle);
    let pin_color = Color32::from_rgb(0x3d, 0x8f, 0xd6);
    for (i, pin) in plan.pins.iter().enumerate() {
        let (dx, dy) = reda_engine::model::geom::rotate(pin.length.0, 0, pin.rotation);
        let a = to_screen(pin.at.x.0, pin.at.y.0);
        let b = to_screen(pin.at.x.0 + dx, pin.at.y.0 + dy);
        painter.line_segment([a, b], Stroke::new(1.5, pin_color));
        if i == 0 {
            painter.circle_filled(a, 3.0, pin_color);
        }
    }
}

const LAYOUTS: [Layout; 6] = [
    Layout::Dil,
    Layout::Quad,
    Layout::Connector,
    Layout::ZigZag,
    Layout::SingleRow,
    Layout::Manual,
];

fn layout_name(layout: Layout) -> String {
    match layout {
        Layout::Dil => tr!("wizard-layout-dil"),
        Layout::Quad => tr!("wizard-layout-quad"),
        Layout::Connector => tr!("wizard-layout-connector"),
        Layout::ZigZag => tr!("wizard-layout-zig-zag"),
        Layout::SingleRow => tr!("wizard-layout-single-row"),
        Layout::Manual => tr!("wizard-layout-manual"),
    }
}

fn target_name(target: Target) -> String {
    match target {
        Target::NewSymbol => tr!("wizard-target-new-symbol"),
        Target::NewUnit => tr!("wizard-target-new-unit"),
        Target::ThisUnit => tr!("wizard-target-this-unit"),
    }
}

fn side_name(side: Option<Side>) -> String {
    match side {
        None => tr!("wizard-side-auto"),
        Some(Side::Left) => tr!("pin-side-left"),
        Some(Side::Right) => tr!("pin-side-right"),
        Some(Side::Top) => tr!("pin-side-top"),
        Some(Side::Bottom) => tr!("pin-side-bottom"),
    }
}
