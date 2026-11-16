//! The Symbol Wizard: a modal dialog over `library.symbol.generate`. It has a layout, a
//! target (this Unit, reopened; a new Unit of this Symbol; or a new Symbol in a Library),
//! the pin grid (number, name, type, side, group) with the column-mapping paste, and a
//! live preview of the layout. Reopening reads the Unit's Pins back as the grid
//! (`wizard::read_unit`); the Command keeps Pin UUIDs by number.
//!
//! Opened from a Part editor (`part` in the call's params), the new Symbol becomes that
//! Part's Symbol once made.

use std::any::Any;

use egui::{Id, Pos2, Rect, Role, ScrollArea, Stroke, Ui, WidgetInfo};
use reda_engine::command::CommandError;
use reda_engine::library::wizard::{self, Layout, Side, SymbolPlan, WizardPin};
use reda_engine::model::geom::Nm;
use reda_engine::model::{PinType, Uuid};
use serde_json::{Value, json};

use super::{PasteMapping, PasteOutcome, initial_library};
use crate::command_dialog::{Outcome, ParamsDialog};
use crate::panels::pins::pin_type_name;
use crate::settings::{layout, strong};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column};

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
        let t = ui.tokens();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width((wide * 0.58).max(360.0));
                self.settings(ui, shell);
                ui.add_space(t.space.section);
                self.grid(ui);
            });
            ui.add_space(t.space.section);
            ui.vertical(|ui| {
                strong(ui, tr!("wizard-preview"));
                preview(ui, self.plan().ok().as_ref());
            });
        });
        if let Err(why) = self.plan() {
            layout::error(ui, why);
        }
        if let Some(error) = &self.error {
            layout::error(ui, error);
        }
        crate::modal::buttons(ui, |ui| {
            let run = ui.add_enabled(
                ready.is_some(),
                widgets::button(tr!("wizard-run")).primary(),
            );
            if run.clicked()
                && let Some(params) = ready
            {
                outcome = Outcome::Run(params.clone());
            }
            if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                outcome = Outcome::Cancel;
            }
        });
        outcome
    }

    fn settings(&mut self, ui: &mut Ui, shell: &mut Shell) {
        widgets::form("wizard-settings").show(ui, |ui| {
            let label = tr!("wizard-layout");
            ui.add(widgets::form_name(&label));
            widgets::dropdown("wizard-layout", layout_name(self.layout))
                .width(super::FIELD_WIDTH)
                .show(ui, |ui| {
                    for l in LAYOUTS {
                        widgets::choice_value(ui, &mut self.layout, l, layout_name(l));
                    }
                })
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.end_row();
            let label = tr!("wizard-pin-length");
            ui.add(widgets::form_name(&label));
            let r = ui.add(widgets::text_field(&mut self.pin_length).width(LENGTH_WIDTH));
            r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
            ui.end_row();
            let label = tr!("wizard-target");
            ui.add(widgets::form_name(&label));
            let mut targets = vec![Target::NewSymbol];
            if self.symbol.is_some() {
                targets = vec![Target::ThisUnit, Target::NewUnit, Target::NewSymbol];
            }
            widgets::dropdown("wizard-target", target_name(self.target))
                .width(super::FIELD_WIDTH)
                .show(ui, |ui| {
                    for t in targets {
                        widgets::choice_value(ui, &mut self.target, t, target_name(t));
                    }
                })
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.end_row();
            if self.target == Target::NewSymbol {
                super::library_picker(
                    ui,
                    shell,
                    Id::NULL.with("wizard-library"),
                    &mut self.library,
                );
                ui.end_row();
                let label = tr!("wizard-name");
                ui.add(widgets::form_name(&label));
                let r = ui.add(widgets::text_field(&mut self.name).width(super::FIELD_WIDTH));
                r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
                ui.end_row();
            }
        });
    }

    fn grid(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            strong(ui, tr!("wizard-pins"));
            if ui.add(widgets::button(tr!("wizard-add-row"))).clicked() {
                let next = self.rows.len() + 1;
                self.rows.push(WizardPin::new(next.to_string(), ""));
            }
            if ui.add(widgets::button(tr!("pin-table-paste"))).clicked() {
                self.paste = Some(PasteMapping::default());
            }
        });
        let mut remove = None;
        ScrollArea::vertical()
            .id_salt("wizard-grid")
            .max_height(GRID_HEIGHT)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                widgets::table("wizard-grid-rows")
                    .columns([
                        Column::auto(),
                        Column::auto(),
                        Column::auto(),
                        Column::auto(),
                        Column::auto(),
                        Column::auto(),
                    ])
                    .show(ui, |table| {
                        table.header(|h| {
                            for key in [
                                "pin-table-number",
                                "pin-table-name",
                                "pin-table-type",
                                "pin-table-side",
                                "pin-table-group",
                            ] {
                                h.text(crate::i18n::translate(key, &[]));
                            }
                            h.empty();
                        });
                        for (i, row) in self.rows.iter_mut().enumerate() {
                            table.row(|cells| {
                                let n = (i + 1).to_string();
                                let cell = |key: &str| {
                                    tr!(
                                        "wizard-cell",
                                        row = n.as_str(),
                                        column = crate::i18n::translate(key, &[])
                                    )
                                };
                                cells.ui(|ui| {
                                    text(
                                        ui,
                                        &mut row.number,
                                        &cell("pin-table-number"),
                                        NUMBER_WIDTH,
                                    );
                                });
                                cells.ui(|ui| {
                                    text(ui, &mut row.name, &cell("pin-table-name"), NAME_WIDTH);
                                });
                                cells.ui(|ui| {
                                    let label = cell("pin-table-type");
                                    widgets::dropdown(
                                        ("wizard-type", i),
                                        pin_type_name(row.electrical_type),
                                    )
                                    .width(TYPE_WIDTH)
                                    .show(ui, |ui| {
                                        for t in PinType::ALL {
                                            widgets::choice_value(
                                                ui,
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
                                });
                                cells.ui(|ui| {
                                    let label = cell("pin-table-side");
                                    widgets::dropdown(("wizard-side", i), side_name(row.side))
                                        .width(SIDE_WIDTH)
                                        .show(ui, |ui| {
                                            for s in [
                                                None,
                                                Some(Side::Left),
                                                Some(Side::Right),
                                                Some(Side::Top),
                                                Some(Side::Bottom),
                                            ] {
                                                widgets::choice_value(
                                                    ui,
                                                    &mut row.side,
                                                    s,
                                                    side_name(s),
                                                );
                                            }
                                        })
                                        .response
                                        .widget_info(|| {
                                            WidgetInfo::labeled(Role::ComboBox, true, label.clone())
                                        });
                                });
                                cells.ui(|ui| {
                                    let mut group = row.group.clone().unwrap_or_default();
                                    text(ui, &mut group, &cell("pin-table-group"), NUMBER_WIDTH);
                                    row.group =
                                        (!group.trim().is_empty()).then(|| group.trim().to_owned());
                                });
                                cells.ui(|ui| {
                                    let what = tr!("wizard-remove-row", row = n.as_str());
                                    let b = ui.add(widgets::button(tr!("paste-map-remove")).flat());
                                    b.widget_info(|| {
                                        WidgetInfo::labeled(Role::Button, true, what.clone())
                                    });
                                    if b.clicked() {
                                        remove = Some(i);
                                    }
                                });
                            });
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

/// How wide the Pin length field is.
const LENGTH_WIDTH: f32 = 60.0;
/// How tall the pin grid is before it scrolls.
const GRID_HEIGHT: f32 = 320.0;
/// The pin grid's column widths: number and group, name, type, side.
const NUMBER_WIDTH: f32 = 48.0;
const NAME_WIDTH: f32 = 100.0;
const TYPE_WIDTH: f32 = 130.0;
const SIDE_WIDTH: f32 = 90.0;
/// The preview's side.
const PREVIEW: f32 = 300.0;
/// The room between the preview's edge and the drawing.
const PREVIEW_MARGIN: f32 = 12.0;
/// The dot that marks Pin 1.
const PIN_ONE_DOT: f32 = 3.0;

fn text(ui: &mut Ui, value: &mut String, label: &str, width: f32) {
    let r = ui.add(widgets::text_field(value).width(width));
    let label = label.to_owned();
    r.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label.clone()));
}

/// The plan drawn to fit the preview: the body and each Pin's line, with Pin 1 marked.
fn preview(ui: &mut Ui, plan: Option<&SymbolPlan>) {
    let t = ui.tokens();
    let size = egui::Vec2::splat(PREVIEW);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let label = tr!("wizard-preview");
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, label.clone()));
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, t.radius.control, t.color.surface.well);
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
    let scale = (rect.width() - 2.0 * PREVIEW_MARGIN) / span;
    let mid = (((min_x + max_x) / 2) as f32, ((min_y + max_y) / 2) as f32);
    let to_screen = |x: i64, y: i64| {
        Pos2::new(
            rect.center().x + (x as f32 - mid.0) * scale,
            rect.center().y - (y as f32 - mid.1) * scale,
        )
    };
    // The drawing's lines are a little heavier than a hairline, as on the canvas.
    let line = t.stroke.hairline * 1.5;
    let stroke = Stroke::new(line, t.color.text.primary);
    let body = Rect::from_two_pos(
        to_screen(plan.body.0.x.0, plan.body.0.y.0),
        to_screen(plan.body.1.x.0, plan.body.1.y.0),
    );
    painter.rect_stroke(body, 0.0, stroke, egui::StrokeKind::Middle);
    let pin_color = t.color.accent.text;
    for (i, pin) in plan.pins.iter().enumerate() {
        let (dx, dy) = reda_engine::model::geom::rotate(pin.length.0, 0, pin.rotation);
        let a = to_screen(pin.at.x.0, pin.at.y.0);
        let b = to_screen(pin.at.x.0 + dx, pin.at.y.0 + dy);
        painter.line_segment([a, b], Stroke::new(line, pin_color));
        if i == 0 {
            painter.circle_filled(a, PIN_ONE_DOT, pin_color);
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
