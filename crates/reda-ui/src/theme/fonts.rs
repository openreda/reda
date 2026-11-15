//! The chrome's fonts (docs/spec/v0.1/design-language.md#type).
//!
//! REDA bundles Inter as the UI face and JetBrains Mono as the monospace face. egui has no
//! weight on a text style, so each weight the type scale uses is a named family of its own
//! ([`REGULAR`], [`MEDIUM`], [`SEMIBOLD`]), beside [`NUMERIC`] for numbers that must line up
//! and [`MONO`]. The names never change: [`resolve`] decides which faces stand behind them.
//!
//! The user may replace either face with an installed family, by name ([`Choice`]). The
//! installed fonts come from a [`Source`] (the app's installed-font index):
//!
//! - **Weights:** each resolves to the weight axis of a variable font, otherwise to the
//!   family's nearest static weight ([`Family::pick`]). Bold is never faked.
//! - **Digits:** the numeric family is the chosen face if its digits are one width,
//!   otherwise the bundled Inter with tabular digits baked in (`fonts/tabular.rs`).
//! - **Missing glyphs:** the chosen face, then the bundled face, then egui's own fonts.
//! - **A family that is not installed, or whose file fails [`validate`],** leaves the
//!   bundled face in place; the [`Report`] says so.

mod check;
#[cfg(test)]
mod tabular;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use egui::epaint::text::{FontTweak, VariationCoords};
use egui::{FontData, FontDefinitions, FontFamily};

pub use check::{Invalid, tabular_digits, validate};

const INTER_REGULAR: &[u8] = include_bytes!("../../fonts/inter/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../fonts/inter/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../fonts/inter/Inter-SemiBold.ttf");
/// Inter Regular with its tabular digits as the default ones (see `fonts/tabular.rs`).
const INTER_TABULAR: &[u8] = include_bytes!("../../fonts/inter/Inter-Tabular.ttf");
const JETBRAINS_MONO: &[u8] =
    include_bytes!("../../fonts/jetbrains-mono/JetBrainsMonoNL-Regular.ttf");

/// The named family of the UI face at regular weight. `FontFamily::Proportional` is bound
/// to the same fonts.
pub const REGULAR: &str = "reda-ui-regular";
pub const MEDIUM: &str = "reda-ui-medium";
pub const SEMIBOLD: &str = "reda-ui-semibold";
/// The named family of numeric recipes: a face whose digits are one width.
pub const NUMERIC: &str = "reda-ui-numeric";
/// The named family of the monospace face. `FontFamily::Monospace` is bound to the same
/// fonts.
pub const MONO: &str = "reda-mono";

/// The bundled faces, as the Preferences name them ("Default (Inter)").
pub const BUNDLED_UI: &str = "Inter";
pub const BUNDLED_MONO: &str = "JetBrains Mono";

/// A weight the type scale uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
}

impl Weight {
    pub const ALL: [Weight; 3] = [Weight::Regular, Weight::Medium, Weight::Semibold];

    /// The OpenType weight class: 400, 500 or 600.
    pub fn class(self) -> u16 {
        match self {
            Weight::Regular => 400,
            Weight::Medium => 500,
            Weight::Semibold => 600,
        }
    }

    /// The named family that carries this weight.
    pub fn family(self) -> &'static str {
        match self {
            Weight::Regular => REGULAR,
            Weight::Medium => MEDIUM,
            Weight::Semibold => SEMIBOLD,
        }
    }
}

/// One face of an installed family, as the installed-font index records it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Face {
    pub file: PathBuf,
    /// The face's index in its file (0 unless the file is a collection).
    pub index: u32,
    /// The OpenType weight class, 1 to 1000 (400 is regular). For a variable font, the
    /// weight of its default instance.
    pub weight: u16,
    /// The lightest and heaviest weight of a variable font's weight axis.
    pub weight_axis: Option<(u16, u16)>,
    /// The OpenType width class, 1 to 9 (5 is normal).
    pub width: u16,
    pub italic: bool,
    /// Every glyph advances by one width.
    pub fixed_pitch: bool,
    /// The face says it covers basic Latin.
    pub latin: bool,
}

impl Face {
    /// The weight nearest to `wanted` that this face can draw.
    fn nearest(&self, wanted: u16) -> u16 {
        match self.weight_axis {
            Some((lightest, heaviest)) if lightest <= heaviest => wanted.clamp(lightest, heaviest),
            _ => self.weight,
        }
    }
}

/// An installed family: every face that goes by its name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Family {
    /// The name as the font files spell it.
    pub name: String,
    pub faces: Vec<Face>,
}

/// The face a weight resolves to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pick<'a> {
    pub face: &'a Face,
    /// The weight the text is drawn at: the wanted one where the family has it.
    pub weight: u16,
    /// The weight is set on the face's weight axis (a variable font).
    pub on_axis: bool,
}

impl Family {
    /// The face that draws `weight`: an upright face of normal width where the family has
    /// one, set on its weight axis if it is variable, otherwise the nearest static weight.
    /// Of two equally near, medium takes the lighter and semibold the heavier. A family
    /// with one weight gives that face for every weight: nothing is emboldened.
    pub fn pick(&self, weight: Weight) -> Option<Pick<'_>> {
        let wanted = weight.class();
        let face = self.faces.iter().min_by_key(|face| {
            let drawn = face.nearest(wanted);
            let heavier = drawn > wanted;
            (
                face.italic,
                face.width.abs_diff(5),
                drawn.abs_diff(wanted),
                // The tie: 400 for a wanted 500, 700 for a wanted 600.
                heavier == (weight != Weight::Semibold),
            )
        })?;
        Some(Pick {
            face,
            weight: face.nearest(wanted),
            on_axis: face.weight_axis.is_some(),
        })
    }

    /// Whether the family's regular face is fixed-pitch: what the monospace list holds.
    pub fn is_fixed_pitch(&self) -> bool {
        self.pick(Weight::Regular)
            .is_some_and(|p| p.face.fixed_pitch)
    }

    /// Whether the family has an upright regular face that covers basic Latin: what the
    /// UI font list holds.
    pub fn has_latin_regular(&self) -> bool {
        self.pick(Weight::Regular)
            .is_some_and(|p| p.weight == 400 && !p.face.italic && p.face.latin)
    }

    /// The weights the family can draw upright, lightest first: each static weight, and
    /// the two ends of a weight axis.
    pub fn weights(&self) -> Vec<u16> {
        let upright = self.faces.iter().any(|f| !f.italic);
        let mut out: Vec<u16> = self
            .faces
            .iter()
            .filter(|f| f.italic != upright)
            .flat_map(|f| match f.weight_axis {
                Some((lightest, heaviest)) => vec![lightest, heaviest],
                None => vec![f.weight],
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// The fonts installed on the machine. The app's installed-font index is one; a test
/// passes a fake. Every method has a default that knows no fonts.
pub trait Source: Send + Sync {
    /// The installed family called `name`, compared case-insensitively.
    fn family(&self, _name: &str) -> Option<Family> {
        None
    }

    /// Every installed family, in alphabetical order.
    fn families(&self) -> Vec<Family> {
        Vec::new()
    }

    /// The bytes of the file `face` is in.
    fn read(&self, _face: &Face) -> Option<Arc<[u8]>> {
        None
    }
}

/// No installed fonts: only the bundled faces.
pub struct NoFonts;

impl Source for NoFonts {}

/// The two font Preferences: a family name each, or `None` for the bundled face.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Choice {
    pub ui: Option<String>,
    pub mono: Option<String>,
}

/// What became of one font Preference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Nothing was chosen: the bundled face.
    Bundled,
    /// The chosen family is in use.
    Installed,
    /// The chosen family is not on this machine: the bundled face is in use.
    NotInstalled,
    /// The chosen family's regular face failed validation: the bundled face is in use.
    Rejected(Invalid),
}

impl Outcome {
    /// Whether the bundled face is standing in for a family the user chose.
    pub fn fell_back(self) -> bool {
        matches!(self, Outcome::NotInstalled | Outcome::Rejected(_))
    }
}

/// What [`resolve`] made of a [`Choice`]: for the Preferences window to show.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Report {
    pub ui: Outcome,
    pub mono: Outcome,
    /// The weights the regular, medium and semibold styles are drawn at.
    pub weights: [u16; 3],
    /// Numeric recipes use the chosen UI face (its digits are one width) and not the
    /// bundled Inter with tabular digits.
    pub numeric_is_chosen: bool,
}

/// Font definitions for egui, and what they stand for.
#[derive(Clone)]
pub struct Resolved {
    pub definitions: FontDefinitions,
    pub report: Report,
}

/// The font keys of the bundled faces in the definitions.
const BUNDLED: [(&str, &[u8]); 5] = [
    ("reda-inter-regular", INTER_REGULAR),
    ("reda-inter-medium", INTER_MEDIUM),
    ("reda-inter-semibold", INTER_SEMIBOLD),
    ("reda-inter-tabular", INTER_TABULAR),
    ("reda-jetbrains-mono", JETBRAINS_MONO),
];

fn bundled_key(weight: Weight) -> &'static str {
    BUNDLED[Weight::ALL.iter().position(|w| *w == weight).unwrap_or(0)].0
}

/// A chosen face ready for egui: validated bytes, and the weight to set on its axis.
struct Loaded {
    bytes: Arc<[u8]>,
    index: u32,
    weight: u16,
    on_axis: bool,
}

impl Loaded {
    fn data(&self) -> FontData {
        let data = FontData::from_blob(Arc::new(Arc::clone(&self.bytes)), self.index);
        if !self.on_axis {
            return data;
        }
        data.tweak(FontTweak {
            coords: VariationCoords::new([(b"wght", f32::from(self.weight))]),
            ..FontTweak::default()
        })
    }
}

/// Reads and validates the face of `family` for `weight`.
fn load(source: &dyn Source, family: &Family, weight: Weight) -> Result<Loaded, Outcome> {
    let pick = family.pick(weight).ok_or(Outcome::NotInstalled)?;
    let bytes = source.read(pick.face).ok_or(Outcome::NotInstalled)?;
    validate(&bytes, pick.face.index).map_err(Outcome::Rejected)?;
    Ok(Loaded {
        bytes,
        index: pick.face.index,
        weight: pick.weight,
        on_axis: pick.on_axis,
    })
}

/// The installed family a Preference names, with its regular face loaded.
fn chosen(source: &dyn Source, name: Option<&str>) -> Result<(Family, Loaded), Outcome> {
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or(Outcome::Bundled)?;
    let family = source.family(name).ok_or(Outcome::NotInstalled)?;
    let regular = load(source, &family, Weight::Regular)?;
    Ok((family, regular))
}

/// The font definitions for `choice`, taking installed families from `source`: the five
/// named families, with egui's `Proportional` and `Monospace` bound like [`REGULAR`] and
/// [`MONO`], so text that names no REDA family is in the same faces.
pub fn resolve(choice: &Choice, source: &dyn Source) -> Resolved {
    let mut fonts = FontDefinitions::default();
    // egui's own fonts end every chain: they carry its symbols and emoji.
    let own = |fonts: &FontDefinitions, family: FontFamily| {
        fonts.families.get(&family).cloned().unwrap_or_default()
    };
    let proportional = own(&fonts, FontFamily::Proportional);
    let monospace = own(&fonts, FontFamily::Monospace);
    for (key, bytes) in BUNDLED {
        fonts
            .font_data
            .insert(key.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let mut chains: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut report = Report {
        ui: Outcome::Bundled,
        mono: Outcome::Bundled,
        weights: Weight::ALL.map(Weight::class),
        numeric_is_chosen: false,
    };

    match chosen(source, choice.ui.as_deref()) {
        Ok((family, regular)) => {
            report.ui = Outcome::Installed;
            report.numeric_is_chosen = tabular_digits(&regular.bytes, regular.index);
            for (i, weight) in Weight::ALL.into_iter().enumerate() {
                // A medium or semibold face that cannot be used leaves the regular one.
                let face = match weight {
                    Weight::Regular => None,
                    _ => load(source, &family, weight).ok(),
                };
                let face = face.as_ref().unwrap_or(&regular);
                report.weights[i] = face.weight;
                let key = format!("reda-chosen-ui-{}", weight.class());
                fonts.font_data.insert(key.clone(), Arc::new(face.data()));
                chains.insert(weight.family(), vec![key, bundled_key(weight).to_owned()]);
            }
            let mut numeric = vec![BUNDLED[3].0.to_owned()];
            if report.numeric_is_chosen {
                numeric.insert(0, format!("reda-chosen-ui-{}", Weight::Regular.class()));
            }
            chains.insert(NUMERIC, numeric);
        }
        Err(outcome) => {
            report.ui = outcome;
            for weight in Weight::ALL {
                chains.insert(weight.family(), vec![bundled_key(weight).to_owned()]);
            }
            chains.insert(NUMERIC, vec![BUNDLED[3].0.to_owned()]);
        }
    }
    let mut mono = vec![BUNDLED[4].0.to_owned()];
    match chosen(source, choice.mono.as_deref()) {
        Ok((_, regular)) => {
            report.mono = Outcome::Installed;
            let key = "reda-chosen-mono".to_owned();
            fonts
                .font_data
                .insert(key.clone(), Arc::new(regular.data()));
            mono.insert(0, key);
        }
        Err(outcome) => report.mono = outcome,
    }
    chains.insert(MONO, mono);

    for (name, mut chain) in chains {
        chain.extend(if name == MONO {
            monospace.iter().cloned()
        } else {
            proportional.iter().cloned()
        });
        match name {
            REGULAR => {
                fonts
                    .families
                    .insert(FontFamily::Proportional, chain.clone());
            }
            MONO => {
                fonts.families.insert(FontFamily::Monospace, chain.clone());
            }
            _ => {}
        }
        fonts.families.insert(FontFamily::Name(name.into()), chain);
    }
    Resolved {
        definitions: fonts,
        report,
    }
}

/// What a context's fonts were resolved from.
#[derive(Clone)]
struct State {
    source: Arc<dyn Source>,
    /// `None` until fonts are installed.
    installed: Option<(Choice, Report)>,
}

fn state_id() -> egui::Id {
    egui::Id::NULL.with("reda-theme-fonts")
}

fn state(ctx: &egui::Context) -> State {
    ctx.data(|d| d.get_temp::<State>(state_id()))
        .unwrap_or(State {
            source: Arc::new(NoFonts),
            installed: None,
        })
}

/// Where `ctx` finds installed families from now on. The fonts already installed are
/// resolved again, in case the new source has a family the old one lacked.
pub fn set_source(ctx: &egui::Context, source: Arc<dyn Source>) {
    let installed = state(ctx).installed;
    ctx.data_mut(|d| {
        d.insert_temp(
            state_id(),
            State {
                source,
                installed: None,
            },
        );
    });
    if let Some((choice, _)) = installed {
        apply(ctx, &choice);
    }
}

/// Installs the fonts for `choice` on `ctx`, unless they are the ones already there.
///
/// egui takes new fonts at the start of the next pass, or at once if the pass has laid out
/// no text yet. So the first call on a context must come before any text that uses REDA's
/// families; later calls can come at any time, because the family names do not change.
pub fn apply(ctx: &egui::Context, choice: &Choice) -> Report {
    let mut state = state(ctx);
    if let Some((installed, report)) = &state.installed
        && installed == choice
    {
        return *report;
    }
    let resolved = resolve(choice, &*state.source);
    ctx.set_fonts(resolved.definitions);
    state.installed = Some((choice.clone(), resolved.report));
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
    resolved.report
}

/// What became of the font Preferences on `ctx`, or `None` before fonts are installed.
pub fn report(ctx: &egui::Context) -> Option<Report> {
    state(ctx).installed.map(|(_, report)| report)
}
