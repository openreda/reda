//! Pending text belongs to the PCB tab. Commit a field on Enter or focus loss, and
//! retain a rejected value so it can be corrected. Escape restores the stored value.
use std::collections::BTreeMap;

use egui::{Key, Role, Ui, WidgetInfo};
use reda_engine::model::{Uuid, geom::Nm};
use serde_json::{Value, json};

use crate::theme::TokensExt as _;
use crate::widgets::Tooltip as _;
use crate::{tr, widgets};

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Field {
    Board(&'static str),
    Via(Uuid, &'static str),
    Layer(Uuid, &'static str),
    Copper(Uuid),
    Sheet(Uuid, &'static str),
}

impl Field {
    pub fn command(self, value: Value) -> (&'static str, Value) {
        match self {
            Self::Via(uuid, key) => ("pcb.via_type.set", json!({"via_type":uuid,(key):value})),
            Self::Board(key) => ("pcb.stackup.set", json!({(key): value})),
            Self::Layer(layer, key) => ("pcb.layer.set", json!({"layer": layer, (key): value})),
            Self::Copper(layer) => (
                "pcb.stackup.set",
                json!({"copper": [{"layer": layer, "thickness": value}]}),
            ),
            Self::Sheet(uuid, key) => (
                "pcb.stackup.set",
                json!({"dielectrics": [{"uuid": uuid, (key): value}]}),
            ),
        }
    }

    fn value(self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        let blank = text.is_empty();
        match self {
            Self::Via(_, "hole" | "diameter")
            | Self::Copper(_)
            | Self::Board("thickness")
            | Self::Sheet(_, "thickness") => {
                if blank && matches!(self, Self::Sheet(..)) {
                    return Ok(Value::Null);
                }
                let length = if text.ends_with(|c: char| c.is_ascii_digit() || c == '.') {
                    format!("{text}mm")
                } else {
                    text.to_owned()
                };
                let parsed = if matches!(self, Self::Copper(_)) {
                    Nm::parse_copper(&length)
                } else {
                    Nm::parse(&length)
                };
                parsed
                    .ok()
                    .filter(|n| n.0 > 0)
                    .map(|n| json!(n.0))
                    .ok_or_else(|| tr!("board-setup-positive"))
            }
            _ if blank => Ok(Value::Null),
            Self::Sheet(_, key @ ("dk" | "df")) => {
                let digits = if key == "dk" { 3 } else { 6 };
                scaled(text, digits)
                    .map(|n| json!(n))
                    .ok_or_else(|| tr!("stackup-electrical-number", places = digits))
            }
            Self::Sheet(_, "test_frequency") => text
                .parse::<u64>()
                .map(|n| json!(n))
                .map_err(|_| tr!("stackup-frequency-integer")),
            _ => Ok(json!(text)),
        }
    }
}

fn scaled(text: &str, places: usize) -> Option<u32> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let fraction = fraction.trim_end_matches('0');
    if !text.bytes().any(|b| b.is_ascii_digit())
        || fraction.len() > places
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return None;
    }
    format!("{whole}{fraction:0<places$}").parse().ok()
}

#[derive(Clone, Default)]
pub(super) struct View {
    pub selected: Option<Uuid>,
    pub apply_construction: Option<super::apply::Form>,
    pub save_construction: Option<super::construction::SaveForm>,
    pub pending: Option<super::types::Pending>,
    pub plane_offer: Option<Uuid>,
    pub plane_picker: Option<crate::pcb::plane::Picker>,
    pub layer_change: Option<super::layer_actions::Pending>,
    drafts: BTreeMap<Field, String>,
    errors: BTreeMap<Field, String>,
}

#[derive(Default)]
pub(super) struct Edits {
    pub commands: Vec<(Option<Field>, &'static str, Value)>,
}
impl Edits {
    pub fn set(&mut self, field: Field, value: Value) {
        let (command, params) = field.command(value);
        self.commands.push((Some(field), command, params));
    }
}

impl View {
    /// Field borders identify the rejected cells. Their messages use the full pane width.
    pub fn problems(&self, ui: &mut Ui, board: bool) {
        let t = ui.tokens();
        for (field, error) in &self.errors {
            if matches!(field, Field::Board(_)) == board {
                ui.label(t.ty.body.rich(error).color(t.color.error.text));
            }
        }
    }

    pub fn settled(&mut self, field: Field, error: Option<String>) {
        if let Some(error) = error {
            self.errors.insert(field, error);
        } else {
            self.drafts.remove(&field);
            self.errors.remove(&field);
        }
    }

    pub fn text(
        &mut self,
        ui: &mut Ui,
        field: Field,
        stored: &str,
        hint: &str,
        label: &str,
        edits: &mut Edits,
    ) -> egui::Response {
        let id = ui.make_persistent_id(field);
        let mut text = self
            .drafts
            .get(&field)
            .map(String::as_str)
            .unwrap_or(stored)
            .to_owned();
        let had_focus = ui.memory(|m| m.has_focus(id));
        let escape = had_focus && ui.input(|i| i.key_pressed(Key::Escape));
        let response = ui.add(
            widgets::text_field(&mut text)
                .id(id)
                .width(ui.available_width())
                .hint(hint)
                .invalid(self.errors.contains_key(&field)),
        );
        response.widget_info(|| WidgetInfo::labeled(Role::TextInput, true, label));
        if escape {
            self.settled(field, None);
            response.surrender_focus();
        } else {
            if response.changed() {
                self.errors.remove(&field);
                if text == stored {
                    self.drafts.remove(&field);
                } else {
                    self.drafts.insert(field, text.clone());
                }
            }
            if response.lost_focus() && self.drafts.contains_key(&field) {
                match field.value(&text) {
                    Ok(value) => edits.set(field, value),
                    Err(error) => {
                        self.errors.insert(field, error);
                    }
                }
            }
        }
        if let Some(error) = self.errors.get(&field) {
            response.tooltip(error)
        } else {
            response
        }
    }
}
