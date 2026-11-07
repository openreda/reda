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
    #[serde(default)]
    pub toe: Option<Length>,
    /// At the lead's inner end (Jh).
    #[serde(default)]
    pub heel: Option<Length>,
    /// Along each side of the lead (Js).
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
    pub size: Option<Length>,
    /// The grid for the outer and inner Pad edges (Z and G), which place the Pads.
    #[serde(default)]
    pub place: Option<Length>,
}

/// A package's tolerances, as the spread between its least and greatest value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComponentParams {
    /// The span over the terminals (CL).
    #[serde(default)]
    pub span: Option<Length>,
    /// The span between the terminals' inner ends (CS), whose default is the RMS of the
    /// span's and twice the terminal length's.
    #[serde(default)]
    pub inner: Option<Length>,
    /// The terminal width (CW).
    #[serde(default)]
    pub width: Option<Length>,
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
    #[serde(default)]
    pub length: Option<Length>,
    /// Each lead Pad's width. Left out, IPC-7351's.
    #[serde(default)]
    pub width: Option<Length>,
    /// The distance between the centres of opposite lead Pads: across x, between the
    /// left and right ones. Left out, IPC-7351's.
    #[serde(default)]
    pub spacing: Option<Length>,
    /// QFP and QFN: the same across y, between the top and bottom Pads.
    #[serde(default)]
    pub spacing_y: Option<Length>,
    /// DFN and QFN: the exposed pad's size. Left out, the package's nominal exposed pad.
    #[serde(default)]
    pub exposed_pad: Option<ExposedPadSize>,
    /// Trim gull-wing Pads so they don't run under the body, and lead Pads so they keep
    /// clear of the exposed pad, with a warning for each trim. On by default.
    #[serde(default = "yes")]
    pub trim: bool,
    /// How much of an exposed pad's area its paste windows cover, in percent: 50 by
    /// default.
    #[serde(default = "default_paste_coverage")]
    pub paste_coverage: u32,
    /// DFN and QFN: the exposed pad's paste windows, as columns and rows. Left out, an
    /// exposed pad of 2.1 × 1.6 mm or more gets about one per 1.5 mm each way, and a
    /// smaller one inherits the board's paste.
    #[serde(default)]
    pub paste_array: Option<PasteArray>,
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

fn yes() -> bool {
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
    #[serde(default)]
    pub courtyard_excess: Option<Length>,
}

/// The `pads` section of a through-hole family's params.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThPadsParams {
    /// The finished hole diameter. Left out, the lead's greatest diagonal plus 0.20 mm.
    #[serde(default)]
    pub hole: Option<Length>,
    /// The Pad diameter. Left out, the hole plus 0.50 mm, less where neighbours are
    /// close.
    #[serde(default)]
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
        let (spacing, field) = if along_y {
            (self.pads.spacing_y, "/pads/spacing_y")
        } else {
            (self.pads.spacing, "/pads/spacing")
        };
        if spacing.is_none() && self.pads.length.is_none() {
            return Ok(span);
        }
        let centres = spacing.map_or(Nm((span.outer.0 + span.inner.0) / 2), Length::nm);
        let length = self.pads.length.map_or(span.pad_length(), Length::nm);
        let span = ipc::Span {
            outer: Nm(centres.0 + length.0),
            inner: Nm(centres.0 - length.0),
        };
        if span.inner.0 <= 0 {
            let field = if spacing.is_some() {
                field
            } else {
                "/pads/length"
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
            paste_array: self.pads.paste_array.map(|a| (a.columns, a.rows)),
            ..Finish::outputs(self.outputs)
        }
    }

    /// Refuses the values that are out of range, and the overrides that don't apply to
    /// a family that takes only `applies`.
    pub fn check(&self, applies: Applies) -> Result<(), CommandError> {
        let refuse = |field: &str, why: &str| Err(CommandError::invalid_params(field, why));
        let pads = self.pads;
        if pads.spacing_y.is_some() && !applies.quad {
            return refuse(
                "/pads/spacing_y",
                "only a package with leads on four sides takes `spacing_y`",
            );
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
        if let Some(array) = pads.paste_array {
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
    /// JSON pointers of the IPC-7351 values these params override.
    fn overrides(&self) -> Vec<String>;

    /// The same params with every IPC-7351 override removed.
    fn compliant(&self) -> Self;
}

/// The pointers of the IPC-7351 values a surface-mount family's sections override.
pub fn smd_overrides(
    goals: &GoalsParams,
    tolerances: &TolerancesParams,
    pads: &PadsParams,
) -> Vec<String> {
    let defaults = TolerancesParams::default();
    let given = [
        ("/goals/fillet/toe", goals.fillet.toe.is_some()),
        ("/goals/fillet/heel", goals.fillet.heel.is_some()),
        ("/goals/fillet/side", goals.fillet.side.is_some()),
        (
            "/tolerances/fabrication",
            tolerances.fabrication != defaults.fabrication,
        ),
        (
            "/tolerances/placement",
            tolerances.placement != defaults.placement,
        ),
        (
            "/tolerances/courtyard_excess",
            tolerances.courtyard_excess.is_some(),
        ),
        (
            "/tolerances/round_off/size",
            tolerances.round_off.size.is_some(),
        ),
        (
            "/tolerances/round_off/place",
            tolerances.round_off.place.is_some(),
        ),
        (
            "/tolerances/component/span",
            tolerances.component.span.is_some(),
        ),
        (
            "/tolerances/component/inner",
            tolerances.component.inner.is_some(),
        ),
        (
            "/tolerances/component/width",
            tolerances.component.width.is_some(),
        ),
        ("/pads/length", pads.length.is_some()),
        ("/pads/width", pads.width.is_some()),
        ("/pads/spacing", pads.spacing.is_some()),
        ("/pads/spacing_y", pads.spacing_y.is_some()),
    ];
    given
        .into_iter()
        .filter(|(_, given)| *given)
        .map(|(pointer, _)| pointer.to_owned())
        .collect()
}

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
            width: None,
            spacing: None,
            spacing_y: None,
            ..*pads
        },
    )
}

/// The pointers of the IPC-7251 values a through-hole family's sections override.
pub fn th_overrides(tolerances: &ThTolerancesParams, pads: &ThPadsParams) -> Vec<String> {
    [
        (
            "/tolerances/courtyard_excess",
            tolerances.courtyard_excess.is_some(),
        ),
        ("/pads/hole", pads.hole.is_some()),
        ("/pads/diameter", pads.diameter.is_some()),
    ]
    .into_iter()
    .filter(|(_, given)| *given)
    .map(|(pointer, _)| pointer.to_owned())
    .collect()
}
