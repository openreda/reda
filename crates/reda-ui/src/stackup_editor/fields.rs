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
pub(super) enum BoardField {
    Thickness,
    Finish,
    MaskColor,
    SilkColor,
}

impl BoardField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Thickness => "thickness",
            Self::Finish => "finish",
            Self::MaskColor => "mask_color",
            Self::SilkColor => "silk_color",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum ViaField {
    Name,
    From,
    To,
    Hole,
    Micro,
    UnusedLayers,
}

impl ViaField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::From => "from",
            Self::To => "to",
            Self::Hole => "hole",
            Self::Micro => "micro",
            Self::UnusedLayers => "unused_layers",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Land {
    Start,
    Middle,
    End,
}

impl Land {
    pub fn key(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum LayerField {
    Name,
    Role,
}

impl LayerField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Role => "role",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum SheetField {
    Name,
    Kind,
    Thickness,
    Material,
    Dk,
    Df,
    TestFrequency,
}

impl SheetField {
    pub fn key(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Kind => "kind",
            Self::Thickness => "thickness",
            Self::Material => "material",
            Self::Dk => "dk",
            Self::Df => "df",
            Self::TestFrequency => "test_frequency",
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum Field {
    Board(BoardField),
    Via(Uuid, ViaField),
    Land(Uuid, Land),
    Layer(Uuid, LayerField),
    Copper(Uuid),
    Sheet(Uuid, SheetField),
}

impl Field {
    pub fn command(self, value: Value) -> (&'static str, Value) {
        match self {
            Self::Land(uuid, _) => (
                "pcb.via_type.set",
                json!({"via_type":uuid,"diameter":value}),
            ),
            Self::Via(uuid, key) => (
                "pcb.via_type.set",
                json!({"via_type":uuid,(key.key()):value}),
            ),
            Self::Board(key) => ("pcb.stackup.set", json!({(key.key()): value})),
            Self::Layer(layer, key) => {
                ("pcb.layer.set", json!({"layer": layer, (key.key()): value}))
            }
            Self::Copper(layer) => (
                "pcb.stackup.set",
                json!({"copper": [{"layer": layer, "thickness": value}]}),
            ),
            Self::Sheet(uuid, key) => (
                "pcb.stackup.set",
                json!({"dielectrics": [{"uuid": uuid, (key.key()): value}]}),
            ),
        }
    }

    fn value(self, text: &str) -> Result<Value, String> {
        let text = text.trim();
        let blank = text.is_empty();
        match self {
            Self::Via(_, ViaField::Hole)
            | Self::Land(..)
            | Self::Copper(_)
            | Self::Board(BoardField::Thickness)
            | Self::Sheet(_, SheetField::Thickness) => {
                if blank
                    && matches!(
                        self,
                        Self::Sheet(..) | Self::Land(_, Land::Middle | Land::End)
                    )
                {
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
            Self::Sheet(_, key @ (SheetField::Dk | SheetField::Df)) => {
                let digits = if key == SheetField::Dk { 3 } else { 6 };
                scaled(text, digits)
                    .map(|n| json!(n))
                    .ok_or_else(|| tr!("stackup-electrical-number", places = digits))
            }
            Self::Sheet(_, SheetField::TestFrequency) => text
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
    pub scale: super::preview::Scale,
    pub section: Option<std::sync::Arc<super::preview::Cached>>,
    pub split_sheet: Option<Uuid>,
    pub apply_construction: Option<super::apply::Form>,
    pub save_construction: Option<super::construction::SaveForm>,
    pub pending: Option<super::types::Pending>,
    pub plane_offer: Option<Uuid>,
    pub plane_picker: Option<crate::pcb::plane::Picker>,
    pub layer_change: Option<super::layer_actions::Pending>,
    drafts: BTreeMap<Field, String>,
    errors: BTreeMap<Field, String>,
}

pub(super) struct Edits {
    document: Uuid,
    pub commands: Vec<(Option<Field>, &'static str, Value)>,
}
impl Edits {
    pub fn new(document: Uuid) -> Self {
        Self {
            document,
            commands: Vec::new(),
        }
    }
    pub fn select(&mut self, entry: Uuid) {
        self.commands.push((
            None,
            super::commands::SELECT,
            json!({"document": self.document, "entry":entry}),
        ));
    }

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
