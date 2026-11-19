//! `pcb.stackup.set`: the board's thickness, copper and dielectric entries, finish and
//! colours; and `pcb.fab_notes.set`, the free text for the Fab Notes, which sits beside
//! them on the Stackup page of Board Setup.

mod sheets;

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, CopperThickness, Kind, Length, Meta,
    Registry, Unavailable,
};
use reda_model::geom::Nm;
use reda_model::{BoardSetup, InDocument, Object, Stackup, StackupEntry, StackupTotals, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{needs_pcb, nullable, pcb};

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(StackupSet);
    registry.add(Info);
    registry.add(sheets::SheetAdd);
    registry.add(sheets::SheetRemove);
    registry.add(FabNotesSet);
}

pub struct StackupSet;

/// The Stackup values to change; those left out keep their value.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StackupSetParams {
    /// The finished thickness you order, such as `"1.6mm"`.
    #[serde(default)]
    pub thickness: Option<Length>,
    /// New copper thicknesses, by copper Layer.
    #[serde(default)]
    pub copper: Vec<CopperParam>,
    /// Sheets to update, by dielectric UUID. Omit each field to retain its value.
    #[serde(default)]
    pub dielectrics: Vec<DielectricParam>,
    /// The surface finish, as free text such as `"ENIG"`; `null` or empty clears it.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub finish: Option<Option<String>>,
    /// The solder mask colour, as free text; `null` or empty clears it.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub mask_color: Option<Option<String>>,
    /// The silkscreen colour, as free text; `null` or empty clears it.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub silk_color: Option<Option<String>>,
}

/// One copper Layer's new thickness.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CopperParam {
    /// The copper Layer's UUID.
    pub layer: Uuid,
    /// Its copper, such as `"1oz"` or `"35um"`; 1 oz is exactly 35 µm.
    pub thickness: CopperThickness,
}

/// A patch to one dielectric. Omitted fields retain their stored values.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DielectricParam {
    /// The dielectric entry's UUID.
    pub uuid: Uuid,
    /// Core or Prepreg.
    pub kind: Option<reda_model::DielectricKind>,
    /// The optional name shown in the Stackup.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub name: Option<Option<String>>,
    /// Finished thickness, or null to share the remainder.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<Length>")]
    pub thickness: Option<Option<Length>>,
    /// The material or glass style as specified by the fab.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<String>")]
    pub material: Option<Option<String>>,
    /// Relative permittivity in milli-units.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u32>")]
    pub dk: Option<Option<u32>>,
    /// Loss tangent in micro-units.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u32>")]
    pub df: Option<Option<u32>>,
    /// The measurement frequency for Dk and Df, in Hz.
    #[serde(default, deserialize_with = "nullable")]
    #[schemars(with = "Option<u64>")]
    pub test_frequency: Option<Option<u64>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StackupSetOutput {
    /// The Stackup as set.
    pub stackup: Stackup,
    /// Its thicknesses added up.
    pub totals: StackupTotals,
    /// Problems that do not stop the edit, such as dielectrics that do not add up to the
    /// thickness.
    pub warnings: Vec<String>,
}

/// A thickness that must be positive.
fn positive(pointer: String, nm: Nm, what: &str) -> Result<Nm, CommandError> {
    if nm <= Nm::ZERO {
        return Err(CommandError::invalid_params(
            &pointer,
            format!("the {what} must be positive"),
        ));
    }
    Ok(nm)
}

/// Free text with nothing in it is no value.
fn text(value: Option<String>) -> Option<String> {
    value.filter(|t| !t.trim().is_empty())
}

impl Command for StackupSet {
    const META: Meta = Meta {
        id: "pcb.stackup.set",
        title: "Set Stackup",
        description: "Sets the ordered thickness, copper thicknesses, dielectric materials and \
                      electrical properties, finish and colours. Omitted fields retain their \
                      values. A null dielectric thickness shares the unassigned thickness. \
                      Refuses a negative remainder and reports the totals and any mismatch \
                      when every sheet has a specified thickness.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = StackupSetParams;
    type Output = StackupSetOutput;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: StackupSetParams,
    ) -> Result<StackupSetOutput, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut stackup = pcb.stackup().clone();
        if let Some(thickness) = p.thickness {
            stackup.thickness = positive("/thickness".into(), thickness.nm(), "thickness")?;
        }
        for (i, copper) in p.copper.into_iter().enumerate() {
            let thickness = positive(
                format!("/copper/{i}/thickness"),
                copper.thickness.nm(),
                "copper thickness",
            )?;
            let entry = stackup.entries.iter_mut().find_map(|e| match e {
                StackupEntry::Copper(c) if c.layer == copper.layer => Some(c),
                _ => None,
            });
            let Some(entry) = entry else {
                return Err(CommandError::invalid_params(
                    &format!("/copper/{i}/layer"),
                    format!("{} is not a copper Layer of the Stackup", copper.layer),
                ));
            };
            entry.thickness = thickness;
        }
        for (i, dielectric) in p.dielectrics.into_iter().enumerate() {
            let entry = stackup.entries.iter_mut().find_map(|e| match e {
                StackupEntry::Dielectric(d) if d.uuid == dielectric.uuid => Some(d),
                _ => None,
            });
            let Some(entry) = entry else {
                return Err(CommandError::invalid_params(
                    &format!("/dielectrics/{i}/uuid"),
                    format!("{} is not a dielectric of the Stackup", dielectric.uuid),
                ));
            };
            if let Some(kind) = dielectric.kind {
                entry.kind = kind;
            }
            if let Some(value) = dielectric.thickness {
                entry.thickness = value
                    .map(|t| {
                        positive(
                            format!("/dielectrics/{i}/thickness"),
                            t.nm(),
                            "dielectric thickness",
                        )
                    })
                    .transpose()?;
            }
            if let Some(value) = dielectric.name {
                entry.name = text(value);
            }
            if let Some(value) = dielectric.material {
                entry.material = text(value);
            }
            if let Some(value) = dielectric.dk {
                entry.dk = value;
            }
            if let Some(value) = dielectric.df {
                entry.df = value;
            }
            if let Some(value) = dielectric.test_frequency {
                entry.test_frequency = value;
            }
        }
        for (value, field) in [
            (p.finish, &mut stackup.finish),
            (p.mask_color, &mut stackup.mask_color),
            (p.silk_color, &mut stackup.silk_color),
        ] {
            if let Some(value) = value {
                *field = text(value);
            }
        }
        let totals = stackup.checked_totals().ok_or_else(|| {
            CommandError::invalid_params(
                "/thickness",
                "the sum of Stackup thicknesses exceeds the supported range",
            )
        })?;
        if let Some(remainder) = totals.remainder
            && remainder < Nm::ZERO
        {
            return Err(CommandError::invalid_params(
                "/thickness",
                format!(
                    "the copper and the dielectrics typed in come to {} nm, more than the \
                     {} nm thickness, so a blank dielectric would be {} nm",
                    (totals.copper + totals.dielectric).0,
                    stackup.thickness.0,
                    remainder.0
                ),
            ));
        }
        let warnings = stackup.mismatch().into_iter().collect();
        cx.tx()?
            .replace(Object::Stackup(InDocument::new(document, stackup.clone())))?;
        Ok(StackupSetOutput {
            stackup,
            totals,
            warnings,
        })
    }
}

struct Info;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InfoParams {
    /// The PCB Document whose Stackup to inspect.
    pub document: Uuid,
}

impl Command for Info {
    const META: Meta = Meta {
        id: "pcb.stackup.info",
        title: "Stackup Information",
        description: "Resolves each entry's physical thickness and reports the symmetry and ordered-thickness checks.",
        category: "pcb",
        icon: "inspect",
        kind: Kind::Query,
    };
    type Params = InfoParams;
    type Output = reda_model::StackupInfo;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: Self::Params) -> Result<Self::Output, CommandError> {
        let pcb = cx.project()?.pcb_document(p.document).ok_or_else(|| {
            CommandError::not_found(format!("no PCB Document has UUID {}", p.document))
        })?;
        Ok(pcb.stackup().info())
    }
}

pub struct FabNotesSet;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FabNotesSetParams {
    /// The free text, kept as typed; `null` or blank clears it.
    pub text: Option<String>,
}

impl Command for FabNotesSet {
    const META: Meta = Meta {
        id: "pcb.fab_notes.set",
        title: "Set Fab Notes Text",
        description: "Sets the board's `fab_notes`: free text for a person at the fab, \
                      copied as typed to the end of the Fab Notes that `output.job.run` \
                      writes. `null` or blank text clears it.",
        category: "pcb",
        icon: "layers",
        kind: Kind::Edit,
    };
    type Params = FabNotesSetParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: FabNotesSetParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let fab_notes = text(p.text);
        if pcb.setup().fab_notes == fab_notes {
            return Ok(());
        }
        let setup = BoardSetup {
            fab_notes,
            ..pcb.setup().clone()
        };
        cx.tx()?
            .replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        Ok(())
    }
}
