//! The Part editor's Commands: the Part's name, Component Type, In BOM and In CPL, its
//! Symbol, its alternate Footprints, its pin-map overrides and its default fields.

use std::collections::BTreeSet;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{ComponentType, Field, PinMap, PinPads, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{check_element_name, edit_part, invalid};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(PartEdit);
    registry.add(PartSymbolSet);
    registry.add(PartFootprintsSet);
    registry.add(PartPinMapSet);
    registry.add(PartFieldsSet);
}

const fn meta(id: &'static str, title: &'static str, description: &'static str) -> Meta {
    Meta {
        id,
        title,
        description,
        category: "library",
        icon: "part_edit",
        kind: Kind::Edit,
    }
}

pub struct PartEdit;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartEditParams {
    /// The open Part.
    pub part: Uuid,
    pub name: Option<String>,
    /// A new Component Type. Changing it resets In BOM and In CPL to the new type's
    /// defaults, unless this edit also sets them.
    pub component_type: Option<ComponentType>,
    /// Whether the Part's Components go into the BOM, by default.
    #[serde(default)]
    pub in_bom: Option<bool>,
    /// Whether the Part's Components go into the CPL, by default.
    #[serde(default)]
    pub in_cpl: Option<bool>,
}

impl Command for PartEdit {
    const META: Meta = meta(
        "library.part.edit",
        "Edit Part",
        "Changes an open Part's name, Component Type, In BOM or In CPL. Omitted fields keep \
         their value. A new Component Type resets In BOM and In CPL to its defaults \
         (Standard: both; Mechanical: In BOM; Net Tie: neither), unless they are given too.",
    );
    type Params = PartEditParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PartEditParams) -> Result<(), CommandError> {
        if let Some(name) = &p.name {
            check_element_name(name).map_err(invalid("/name"))?;
        }
        edit_part(cx, p.part, |part| {
            if let Some(name) = p.name {
                part.name = name;
            }
            if let Some(component_type) = p.component_type
                && component_type != part.component_type
            {
                part.component_type = component_type;
                part.in_bom = component_type.default_in_bom();
                part.in_cpl = component_type.default_in_cpl();
            }
            if let Some(in_bom) = p.in_bom {
                part.in_bom = in_bom;
            }
            if let Some(in_cpl) = p.in_cpl {
                part.in_cpl = in_cpl;
            }
            Ok(())
        })
    }
}

pub struct PartSymbolSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartSymbolSetParams {
    /// The open Part.
    pub part: Uuid,
    /// The Symbol by UUID, in any registered Library, or `null` for none (Mechanical
    /// Parts only). The Library check reports a Symbol that cannot be found.
    pub symbol: Option<Uuid>,
}

impl Command for PartSymbolSet {
    const META: Meta = meta(
        "library.part.symbol.set",
        "Set Part Symbol",
        "Picks the Symbol of an open Part, or none with `null`.",
    );
    type Params = PartSymbolSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PartSymbolSetParams) -> Result<(), CommandError> {
        edit_part(cx, p.part, |part| {
            part.symbol = p.symbol;
            Ok(())
        })
    }
}

pub struct PartFootprintsSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartFootprintsSetParams {
    /// The open Part.
    pub part: Uuid,
    /// The alternate Footprints by UUID, in order; the first is the default.
    pub footprints: Vec<Uuid>,
}

impl Command for PartFootprintsSet {
    const META: Meta = meta(
        "library.part.footprints.set",
        "Set Part Footprints",
        "Sets the ordered alternate Footprints of an open Part; the first is the default. \
         Each Footprint may appear once.",
    );
    type Params = PartFootprintsSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PartFootprintsSetParams) -> Result<(), CommandError> {
        let mut seen = BTreeSet::new();
        if let Some(i) = p.footprints.iter().position(|f| !seen.insert(*f)) {
            return Err(CommandError::invalid_params(
                &format!("/footprints/{i}"),
                format!("Footprint {} is listed twice", p.footprints[i]),
            ));
        }
        edit_part(cx, p.part, |part| {
            part.footprints = p.footprints;
            Ok(())
        })
    }
}

pub struct PartPinMapSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartPinMapSetParams {
    /// The open Part.
    pub part: Uuid,
    /// Pins mapped to Pads other than their own number, or to several.
    #[serde(default)]
    pub pins: Vec<PinPads>,
    /// Pad numbers deliberately left unconnected.
    #[serde(default)]
    pub unconnected: Vec<String>,
}

impl Command for PartPinMapSet {
    const META: Meta = meta(
        "library.part.pin_map.set",
        "Set Pin-Pad Map",
        "Replaces an open Part's pin-map overrides. A Pin connects to the Pads with its own \
         number unless `pins` maps it to an explicit list of Pad numbers; `unconnected` \
         lists Pads that belong to no Pin. Empty lists restore the implicit map.",
    );
    type Params = PartPinMapSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PartPinMapSetParams) -> Result<(), CommandError> {
        let map = pin_map(p.pins, p.unconnected)?;
        edit_part(cx, p.part, |part| {
            part.pin_map = map;
            Ok(())
        })
    }
}

/// The overrides checked and in canonical order.
fn pin_map(mut pins: Vec<PinPads>, mut unconnected: Vec<String>) -> Result<PinMap, CommandError> {
    let mut seen_pins = BTreeSet::new();
    let mut mapped = BTreeSet::new();
    for (i, override_) in pins.iter().enumerate() {
        if override_.pin.is_empty() {
            return Err(CommandError::invalid_params(
                &format!("/pins/{i}/pin"),
                "the Pin number is empty",
            ));
        }
        if !seen_pins.insert(override_.pin.clone()) {
            return Err(CommandError::invalid_params(
                &format!("/pins/{i}/pin"),
                format!("Pin {} is mapped twice", override_.pin),
            ));
        }
        if override_.pads.is_empty() {
            return Err(CommandError::invalid_params(
                &format!("/pins/{i}/pads"),
                "a Pin override maps to at least one Pad",
            ));
        }
        mapped.extend(override_.pads.iter().cloned());
    }
    let mut seen_pads = BTreeSet::new();
    for (i, pad) in unconnected.iter().enumerate() {
        if !seen_pads.insert(pad) {
            return Err(CommandError::invalid_params(
                &format!("/unconnected/{i}"),
                format!("Pad {pad} is listed twice"),
            ));
        }
        if mapped.contains(pad) {
            return Err(CommandError::invalid_params(
                &format!("/unconnected/{i}"),
                format!("Pad {pad} is both mapped to a Pin and unconnected"),
            ));
        }
    }
    pins.sort_by(|a, b| a.pin.cmp(&b.pin));
    unconnected.sort();
    Ok(PinMap { pins, unconnected })
}

pub struct PartFieldsSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartFieldsSetParams {
    /// The open Part.
    pub part: Uuid,
    pub value: Option<String>,
    pub description: Option<String>,
    pub datasheet: Option<String>,
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    pub keywords: Option<String>,
    /// Replaces every free-form field, in this order. Names are unique, ignoring case,
    /// and differ from the built-in fields.
    pub custom: Option<Vec<Field>>,
}

/// The built-in field names, which free-form fields may not reuse.
const BUILT_IN: [&str; 6] = [
    "Value",
    "Description",
    "Datasheet",
    "Manufacturer",
    "MPN",
    "Keywords",
];

impl Command for PartFieldsSet {
    const META: Meta = meta(
        "library.part.fields.set",
        "Set Part Fields",
        "Sets an open Part's default fields, which are copied onto each Component placed \
         from it: the built-in Value, Description, Datasheet, Manufacturer, MPN and \
         Keywords (an empty string clears one), and with `custom` the whole list of \
         free-form fields. Omitted fields keep their value.",
    );
    type Params = PartFieldsSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        cx.elements()
    }

    fn run(&self, cx: &mut Context<'_>, p: PartFieldsSetParams) -> Result<(), CommandError> {
        for (pointer, value) in [
            ("/value", &p.value),
            ("/description", &p.description),
            ("/datasheet", &p.datasheet),
            ("/manufacturer", &p.manufacturer),
            ("/mpn", &p.mpn),
            ("/keywords", &p.keywords),
        ] {
            if let Some(value) = value {
                CommandError::check_text(pointer, value)?;
            }
        }
        for (i, field) in p.custom.iter().flatten().enumerate() {
            CommandError::check_text(&format!("/custom/{i}/value"), &field.value)?;
        }
        if let Some(custom) = &p.custom {
            let mut seen = BTreeSet::new();
            for (i, field) in custom.iter().enumerate() {
                let key = field.name.trim().to_lowercase();
                let pointer = format!("/custom/{i}/name");
                if key.is_empty() {
                    return Err(CommandError::invalid_params(&pointer, "the name is empty"));
                }
                if BUILT_IN.iter().any(|b| b.to_lowercase() == key) {
                    return Err(CommandError::invalid_params(
                        &pointer,
                        format!("{} is a built-in field", field.name),
                    ));
                }
                if !seen.insert(key) {
                    return Err(CommandError::invalid_params(
                        &pointer,
                        format!("there is already a field called {}", field.name),
                    ));
                }
            }
        }
        edit_part(cx, p.part, |part| {
            let fields = &mut part.fields;
            for (slot, value) in [
                (&mut fields.value, p.value),
                (&mut fields.description, p.description),
                (&mut fields.datasheet, p.datasheet),
                (&mut fields.manufacturer, p.manufacturer),
                (&mut fields.mpn, p.mpn),
                (&mut fields.keywords, p.keywords),
            ] {
                if let Some(value) = value {
                    *slot = value;
                }
            }
            if let Some(custom) = p.custom {
                fields.custom = custom;
            }
            Ok(())
        })
    }
}
