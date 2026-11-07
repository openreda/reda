//! The param sections every generator shares: `goals`, `tolerances`, `pads` and
//! `outputs`. The target (`library`, `name`, `description`) and the package sit at the top
//! level of each family's params.
//!
//! Defaults are fixed here, never read from Preferences, so one Command gives every user
//! the same Footprint. A value that REDA sets the same way for every package (the board
//! tolerances, the Pads' look, the outputs) is written into the params with its default,
//! so the stored params say what the Footprint was made with. A value the IPC-7351
//! equations or tables give for the package (fillet goals, courtyard excess, round-off,
//! component tolerances, Pad size and spacing) is left out (`null`) to be calculated, and
//! given only to override it.

use reda_command::{CommandError, Length, RatioParam};
use reda_model::Ratio;
use reda_model::geom::Nm;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ipc::{self, Density, Goals};
use super::pattern::{Finish, LeadShape};

/// The `goals` section of a surface-mount family's params.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalsParams {
    /// The IPC-7351 density level: `nominal` (level B, the default), `most` (A, larger
    /// Pads and courtyard) or `least` (C, smaller). It sets the name's last letter (`N`,
    /// `M` or `L`).
    #[serde(default)]
    pub density: Density,
    /// The solder-fillet goals. Each left out takes the IPC-7351 value for the package
    /// family and density.
    #[serde(default)]
    pub fillet: FilletParams,
}

/// Solder-fillet goals, each from the lead's edge; negative shrinks the Pad.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FilletParams {
    /// At the lead's outer end (Jt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toe: Option<Length>,
    /// At the lead's inner end (Jh).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heel: Option<Length>,
    /// Along each side of the lead (Js).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Length>,
}

/// The `tolerances` section of a surface-mount family's params.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TolerancesParams {
    /// The board fabrication tolerance (F), 0.10 mm by default.
    #[serde(default = "default_fabrication")]
    pub fabrication: Length,
    /// The placement tolerance (P), 0.05 mm by default.
    #[serde(default = "default_placement")]
    pub placement: Length,
    /// The courtyard's clearance around the Pads and body. Left out, IPC-7351's for the
    /// family and density.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courtyard_excess: Option<Length>,
    #[serde(default)]
    pub round_off: RoundOffParams,
    /// The package's own tolerances, calculated from its dimension ranges unless given.
    #[serde(default)]
    pub component: ComponentParams,
}

impl Default for TolerancesParams {
    fn default() -> Self {
        TolerancesParams {
            fabrication: default_fabrication(),
            placement: default_placement(),
            courtyard_excess: None,
            round_off: RoundOffParams::default(),
            component: ComponentParams::default(),
        }
    }
}

fn default_fabrication() -> Length {
    Length(ipc::FABRICATION)
}

fn default_placement() -> Length {
    Length(ipc::PLACEMENT)
}

/// The grids Pad sizes and placements are rounded to. Left out, 0.05 mm, or 0.02 mm on
/// chips smaller than 1608.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoundOffParams {
    /// The grid for the Pad width (X).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Length>,
    /// The grid for the outer and inner Pad edges (Z and G), which place the Pads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Length>,
}

/// A package's tolerances, as the spread between its least and greatest value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentParams {
    /// The span over the terminals (CL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Length>,
    /// The span between the terminals' inner ends (CS), whose default is the RMS of the
    /// span's and twice the terminal length's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner: Option<Length>,
    /// The terminal width (CW).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Length>,
    /// QFP and QFN: the span over the top and bottom terminals, when it differs from
    /// `span`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_y: Option<Length>,
    /// QFP and QFN: the inner span of the top and bottom terminals, when it differs
    /// from `inner`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_y: Option<Length>,
}

/// The `pads` section of a surface-mount family's params.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PadsParams {
    /// The Pads' shape, `rounded_rect` by default.
    #[serde(default)]
    pub shape: LeadShape,
    /// A rounded Pad's corners, as a share of half its shorter side: 25% by default.
    #[serde(default = "default_corner_ratio")]
    pub corner_ratio: RatioParam,
    /// The largest corner radius, 0.25 mm by default; larger Pads get a smaller ratio.
    #[serde(default = "default_max_corner_radius")]
    pub max_corner_radius: Length,
    /// Each lead Pad's length, heel to toe. Left out, IPC-7351's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<Length>,
    /// Each lead Pad's width. Left out, IPC-7351's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Length>,
    /// The distance between the centres of opposite lead Pads: across x, between the
    /// left and right ones. Left out, IPC-7351's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<Length>,
    /// QFP and QFN: the same across y, between the top and bottom Pads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing_y: Option<Length>,
    /// QFP and QFN: the top and bottom Pads' length, when it differs from `length`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_y: Option<Length>,
    /// DFN and QFN: the exposed pad's size. Left out, the package's nominal exposed pad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposed_pad: Option<ExposedPadSize>,
    /// Trim gull-wing Pads so they don't run under the body, and lead Pads so they keep
    /// clear of the exposed pad, with a warning for each trim. On by default.
    #[serde(default = "yes")]
    pub trim: bool,
    /// How much of an exposed pad's area its paste windows cover, in percent: 50 by
    /// default.
    #[serde(default = "default_paste_coverage")]
    pub paste_coverage: u32,
    /// A package with an exposed pad: its paste windows, as columns and rows, or
    /// `inherit` for none (the board's paste). Left out, an exposed pad of 2.1 × 1.6 mm
    /// or more gets about one per 1.5 mm each way, and a smaller one inherits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paste_array: Option<PasteSplit>,
}

impl Default for PadsParams {
    fn default() -> Self {
        PadsParams {
            shape: LeadShape::default(),
            corner_ratio: default_corner_ratio(),
            max_corner_radius: default_max_corner_radius(),
            length: None,
            width: None,
            spacing: None,
            spacing_y: None,
            length_y: None,
            exposed_pad: None,
            trim: true,
            paste_coverage: default_paste_coverage(),
            paste_array: None,
        }
    }
}

fn default_corner_ratio() -> RatioParam {
    RatioParam(Ratio::percent(25))
}

fn default_max_corner_radius() -> Length {
    Length(Nm(250_000))
}

fn default_paste_coverage() -> u32 {
    50
}

pub(crate) fn yes() -> bool {
    true
}

/// An exposed pad's size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExposedPadSize {
    /// Along x (E2).
    pub width: Length,
    /// Along y (D2).
    pub length: Length,
}

/// An exposed pad's paste: an array of windows, or none of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum PasteSplit {
    Array(PasteArray),
    Keyword(PasteKeyword),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PasteKeyword {
    /// No windows: the exposed pad inherits the board's paste.
    Inherit,
}

/// An array of paste windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PasteArray {
    /// Along x.
    pub columns: u32,
    /// Along y.
    pub rows: u32,
}

/// The `outputs` section: which outlines are drawn, and how wide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputsParams {
    /// Draw the Silkscreen outline. On by default.
    #[serde(default = "yes")]
    pub silkscreen: bool,
    /// 0.12 mm by default.
    #[serde(default = "default_silkscreen_width")]
    pub silkscreen_width: Length,
    /// Draw the courtyard. On by default.
    #[serde(default = "yes")]
    pub courtyard: bool,
    /// 0.05 mm by default.
    #[serde(default = "default_courtyard_width")]
    pub courtyard_width: Length,
    /// Draw the Assembly outline. On by default.
    #[serde(default = "yes")]
    pub assembly: bool,
    /// 0.10 mm by default.
    #[serde(default = "default_assembly_width")]
    pub assembly_width: Length,
}

impl Default for OutputsParams {
    fn default() -> Self {
        OutputsParams {
            silkscreen: true,
            silkscreen_width: default_silkscreen_width(),
            courtyard: true,
            courtyard_width: default_courtyard_width(),
            assembly: true,
            assembly_width: default_assembly_width(),
        }
    }
}

fn default_silkscreen_width() -> Length {
    Length(Nm(120_000))
}

fn default_courtyard_width() -> Length {
    Length(Nm(50_000))
}

fn default_assembly_width() -> Length {
    Length(Nm(100_000))
}

/// The `tolerances` section of a through-hole family's params.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThTolerancesParams {
    /// The courtyard's clearance around the Pads and body. Left out, 0.25 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub courtyard_excess: Option<Length>,
}

/// The `pads` section of a through-hole family's params.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThPadsParams {
    /// The finished hole diameter. Left out, the lead's greatest diagonal plus 0.20 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole: Option<Length>,
    /// The Pad diameter. Left out, the hole plus 0.50 mm, less where neighbours are
    /// close.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<Length>,
}

/// What a surface-mount family may take in its `pads` section beyond the shared fields.
#[derive(Clone, Copy, Debug, Default)]
pub struct Applies {
    /// Leads on four sides, so `spacing_y`.
    pub quad: bool,
    /// An exposed pad, so `exposed_pad` and `paste_array`.
    pub exposed_pad: bool,
}

/// A surface-mount family's sections.
#[derive(Clone, Copy, Debug)]
pub struct Smd<'a> {
    pub goals: &'a GoalsParams,
    pub tolerances: &'a TolerancesParams,
    pub pads: &'a PadsParams,
    pub outputs: &'a OutputsParams,
}

impl Smd<'_> {
    /// The goals IPC-7351 gives (`table`) for the top and bottom leads of a
    /// four-sided package, with every override applied.
    pub fn goals_y(&self, table: Goals) -> Goals {
        let c = &self.tolerances.component;
        Goals {
            span_tolerance: c.span_y.or(c.span).map(Length::nm),
            inner_tolerance: c.inner_y.or(c.inner).map(Length::nm),
            ..self.goals(table)
        }
    }

    /// The goals IPC-7351 gives (`table`), with every override applied.
    pub fn goals(&self, table: Goals) -> Goals {
        let or = |given: Option<Length>, default: Nm| given.map_or(default, Length::nm);
        let (f, t) = (&self.goals.fillet, self.tolerances);
        Goals {
            toe: or(f.toe, table.toe),
            heel: or(f.heel, table.heel),
            side: or(f.side, table.side),
            courtyard: or(t.courtyard_excess, table.courtyard),
            round_off: or(t.round_off.size, table.round_off),
            place: or(t.round_off.place, table.place),
            fabrication: t.fabrication.nm(),
            placement: t.placement.nm(),
            span_tolerance: t.component.span.map(Length::nm),
            inner_tolerance: t.component.inner.map(Length::nm),
            width_tolerance: t.component.width.map(Length::nm),
        }
    }

    /// `span` with the Pad `length` and the `spacing` (or `spacing_y` when `along_y`)
    /// overrides applied.
    pub fn span(&self, span: ipc::Span, along_y: bool) -> Result<ipc::Span, CommandError> {
        let (spacing, field, length, length_field) = if along_y {
            (
                self.pads.spacing_y,
                "/pads/spacing_y",
                self.pads.length_y.or(self.pads.length),
                "/pads/length_y",
            )
        } else {
            (
                self.pads.spacing,
                "/pads/spacing",
                self.pads.length,
                "/pads/length",
            )
        };
        if spacing.is_none() && length.is_none() {
            return Ok(span);
        }
        let centres = spacing.map_or(Nm((span.outer.0 + span.inner.0) / 2), Length::nm);
        let length = length.map_or(span.pad_length(), Length::nm);
        let span = ipc::Span {
            outer: Nm(centres.0 + length.0),
            inner: Nm(centres.0 - length.0),
        };
        if span.inner.0 <= 0 {
            let field = if spacing.is_some() {
                field
            } else {
                length_field
            };
            return Err(CommandError::invalid_params(
                field,
                "the Pads are too long for their spacing: opposite Pads would meet",
            ));
        }
        Ok(span)
    }

    /// The lead Pad width IPC-7351 gives (`ipc`), or its override.
    pub fn pad_width(&self, ipc: Nm) -> Nm {
        self.pads.width.map_or(ipc, Length::nm)
    }

    /// The Pads' look and the outputs.
    pub fn finish(&self) -> Finish {
        Finish {
            shape: self.pads.shape,
            corner_ratio: self.pads.corner_ratio.ratio(),
            max_corner_radius: self.pads.max_corner_radius.nm(),
            paste_coverage: self.pads.paste_coverage,
            paste_array: self.pads.paste_array,
            ..Finish::outputs(self.outputs)
        }
    }

    /// Refuses the values that are out of range, and the overrides that don't apply to
    /// a family that takes only `applies`.
    pub fn check(&self, applies: Applies) -> Result<(), CommandError> {
        let refuse = |field: &str, why: &str| Err(CommandError::invalid_params(field, why));
        let pads = self.pads;
        let c = &self.tolerances.component;
        for (field, given) in [
            ("/pads/spacing_y", pads.spacing_y.is_some()),
            ("/pads/length_y", pads.length_y.is_some()),
            ("/tolerances/component/span_y", c.span_y.is_some()),
            ("/tolerances/component/inner_y", c.inner_y.is_some()),
        ] {
            if given && !applies.quad {
                return refuse(
                    field,
                    "only a package with leads on four sides takes this value",
                );
            }
        }
        if !applies.exposed_pad {
            if pads.exposed_pad.is_some() {
                return refuse("/pads/exposed_pad", "the package has no exposed pad");
            }
            if pads.paste_array.is_some() {
                return refuse("/pads/paste_array", "the package has no exposed pad");
            }
        }
        if !(1..=100).contains(&pads.paste_coverage) {
            return refuse("/pads/paste_coverage", "the coverage must be 1 to 100%");
        }
        if let Some(PasteSplit::Array(array)) = pads.paste_array {
            if array.columns == 0 {
                return refuse("/pads/paste_array/columns", "give at least one column");
            }
            if array.rows == 0 {
                return refuse("/pads/paste_array/rows", "give at least one row");
            }
        }
        if pads.corner_ratio.ratio() > Ratio::FULL {
            return refuse("/pads/corner_ratio", "the ratio must be at most 100%");
        }
        let positive = [
            ("/pads/length", pads.length),
            ("/pads/width", pads.width),
            ("/pads/spacing", pads.spacing),
            ("/pads/spacing_y", pads.spacing_y),
            ("/pads/length_y", pads.length_y),
            ("/pads/exposed_pad/width", pads.exposed_pad.map(|e| e.width)),
            (
                "/pads/exposed_pad/length",
                pads.exposed_pad.map(|e| e.length),
            ),
            ("/tolerances/round_off/size", self.tolerances.round_off.size),
            (
                "/tolerances/round_off/place",
                self.tolerances.round_off.place,
            ),
        ];
        for (field, value) in positive {
            if value.is_some_and(|v| v.nm().0 <= 0) {
                return refuse(field, "the value must be greater than zero");
            }
        }
        let not_negative = [
            ("/tolerances/fabrication", Some(self.tolerances.fabrication)),
            ("/tolerances/placement", Some(self.tolerances.placement)),
            (
                "/tolerances/courtyard_excess",
                self.tolerances.courtyard_excess,
            ),
            ("/tolerances/component/span", self.tolerances.component.span),
            (
                "/tolerances/component/inner",
                self.tolerances.component.inner,
            ),
            (
                "/tolerances/component/width",
                self.tolerances.component.width,
            ),
            ("/tolerances/component/span_y", c.span_y),
            ("/tolerances/component/inner_y", c.inner_y),
            ("/pads/max_corner_radius", Some(pads.max_corner_radius)),
        ];
        for (field, value) in not_negative {
            if value.is_some_and(|v| v.nm().0 < 0) {
                return refuse(field, "the value must not be negative");
            }
        }
        check_outputs(self.outputs)
    }
}

/// Refuses outline widths that are not positive.
pub fn check_outputs(outputs: &OutputsParams) -> Result<(), CommandError> {
    for (field, width) in [
        ("/outputs/silkscreen_width", outputs.silkscreen_width),
        ("/outputs/courtyard_width", outputs.courtyard_width),
        ("/outputs/assembly_width", outputs.assembly_width),
    ] {
        if width.nm().0 <= 0 {
            return Err(CommandError::invalid_params(
                field,
                "the width must be greater than zero",
            ));
        }
    }
    Ok(())
}

/// The sections of a family's params, for the non-compliance warning.
pub trait Sections: Clone {
    /// JSON pointers of the values IPC-7351 (or IPC-7251) calculates, which a caller
    /// may override.
    const IPC_VALUES: &'static [&'static str];

    /// Those of [`Sections::IPC_VALUES`] that IPC-7351 derives from the others: a
    /// change to another one changes them too, so they count as overridden only when
    /// they differ from what the others give.
    const DERIVED: &'static [&'static str];

    /// The same params with every IPC-7351 override removed.
    fn compliant(&self) -> Self;

    /// The same params with the overrides of [`Sections::DERIVED`] removed.
    fn underived(&self) -> Self;

    /// The same params with every value of the sections written out as `resolved`.
    fn resolved_sections(&self, resolved: super::Resolved) -> Self;

    /// The same params with the name and description given.
    fn named(&self, name: String, description: String) -> Self;
}

/// The surface-mount families' IPC-7351 values.
pub const SMD_IPC_VALUES: &[&str] = &[
    "/goals/fillet/toe",
    "/goals/fillet/heel",
    "/goals/fillet/side",
    "/tolerances/fabrication",
    "/tolerances/placement",
    "/tolerances/courtyard_excess",
    "/tolerances/round_off/size",
    "/tolerances/round_off/place",
    "/tolerances/component/span",
    "/tolerances/component/inner",
    "/tolerances/component/width",
    "/tolerances/component/span_y",
    "/tolerances/component/inner_y",
    "/pads/length",
    "/pads/length_y",
    "/pads/width",
    "/pads/spacing",
    "/pads/spacing_y",
];

/// The surface-mount values IPC-7351 derives from the goals and tolerances.
pub const SMD_DERIVED: &[&str] = &[
    "/pads/length",
    "/pads/length_y",
    "/pads/width",
    "/pads/spacing",
    "/pads/spacing_y",
];

/// The through-hole values derived from the others: the Pad from its hole.
pub const TH_DERIVED: &[&str] = &["/pads/diameter"];

/// The through-hole families' IPC-7251 values.
pub const TH_IPC_VALUES: &[&str] = &[
    "/tolerances/courtyard_excess",
    "/pads/hole",
    "/pads/diameter",
];

/// A surface-mount family's sections with the IPC-7351 overrides removed.
pub fn smd_compliant(
    goals: &GoalsParams,
    pads: &PadsParams,
) -> (GoalsParams, TolerancesParams, PadsParams) {
    (
        GoalsParams {
            density: goals.density,
            fillet: FilletParams::default(),
        },
        TolerancesParams::default(),
        PadsParams {
            length: None,
            length_y: None,
            width: None,
            spacing: None,
            spacing_y: None,
            ..*pads
        },
    )
}

/// What a surface-mount plan used for the values its params may leave to be
/// calculated, so they can be written out resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmdResolved {
    /// The goals, with every override applied.
    pub goals: Goals,
    /// `CL`, `CS` and `CW`.
    pub tolerances: (Nm, Nm, Nm),
    /// The span of the left and right (or only) lead Pads, before any trim.
    pub span: ipc::Span,
    /// A four-sided package's `CL` and `CS` along y, and its top and bottom span.
    pub y: Option<((Nm, Nm), ipc::Span)>,
    /// The lead Pads' width.
    pub pad_width: Nm,
    /// The exposed pad's width (x) and length (y), and its paste.
    pub exposed_pad: Option<(Nm, Nm, PasteSplit)>,
}

/// What a through-hole plan used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThResolved {
    pub courtyard_excess: Nm,
    pub hole: Nm,
    pub diameter: Nm,
}

impl SmdResolved {
    /// The sections `goals`, `tolerances` and `pads` with every value written out.
    pub fn sections(
        &self,
        goals: &GoalsParams,
        pads: &PadsParams,
    ) -> (GoalsParams, TolerancesParams, PadsParams) {
        let g = self.goals;
        let l = |nm: Nm| Some(Length(nm));
        let (span_y, inner_y, spacing_y, length_y) = match self.y {
            Some(((cl, cs), span)) => (
                l(cl),
                l(cs),
                l(Nm((span.outer.0 + span.inner.0) / 2)),
                l(span.pad_length()),
            ),
            None => (None, None, None, None),
        };
        (
            GoalsParams {
                density: goals.density,
                fillet: FilletParams {
                    toe: l(g.toe),
                    heel: l(g.heel),
                    side: l(g.side),
                },
            },
            TolerancesParams {
                fabrication: Length(g.fabrication),
                placement: Length(g.placement),
                courtyard_excess: l(g.courtyard),
                round_off: RoundOffParams {
                    size: l(g.round_off),
                    place: l(g.place),
                },
                component: ComponentParams {
                    span: l(self.tolerances.0),
                    inner: l(self.tolerances.1),
                    width: l(self.tolerances.2),
                    span_y,
                    inner_y,
                },
            },
            PadsParams {
                length: l(self.span.pad_length()),
                width: l(self.pad_width),
                spacing: l(Nm((self.span.outer.0 + self.span.inner.0) / 2)),
                spacing_y,
                length_y,
                exposed_pad: self.exposed_pad.map(|(width, length, _)| ExposedPadSize {
                    width: Length(width),
                    length: Length(length),
                }),
                paste_array: self.exposed_pad.map(|(_, _, paste)| paste),
                ..*pads
            },
        )
    }
}

impl ThResolved {
    /// The sections `tolerances` and `pads` with every value written out.
    pub fn sections(&self) -> (ThTolerancesParams, ThPadsParams) {
        (
            ThTolerancesParams {
                courtyard_excess: Some(Length(self.courtyard_excess)),
            },
            ThPadsParams {
                hole: Some(Length(self.hole)),
                diameter: Some(Length(self.diameter)),
            },
        )
    }
}
