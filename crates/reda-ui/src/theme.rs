//! The chrome's single styling source (docs/spec/v0.1/design-language.md, ADR-0015).
//!
//! Three layers: a private [palette] of raw values; [`Tokens`], which names each value by
//! its job; and the recipes of the REDA widgets, which pick tokens and hold no numbers.
//! Code reads the tokens through [`TokensExt::tokens`] (`ui.tokens()`). egui's `Style` is
//! generated from them by [`style`], and nothing else sets a style or visuals.
//!
//! The look is Paper: warm neutral surfaces, one blue accent and three semantic colours,
//! in light and dark, following the OS by default.

#[cfg(test)]
mod contrast;
pub mod fonts;
mod palette;
mod style;
#[cfg(test)]
mod tables;
mod tokens;

use std::sync::{Arc, OnceLock};

use egui::{Color32, Theme};
use reda_engine::preferences::{self, Preferences};

pub use style::style;
pub use tokens::{
    Colors, Density, Easing, Elevation, Families, Fill, IconColor, IconSize, Metric, Motion,
    Opacity, Radius, Settings, Space, StrokeColor, StrokeWidth, Surface, Text, TextColor, Tokens,
    Tone, Transition, Type,
};

/// `top`, which may be translucent, laid over `under`: how a hover or press fill goes over
/// a control, a tint over a surface. The only colour blending the chrome does.
pub fn over(top: Color32, under: Color32) -> Color32 {
    // egui keeps colours premultiplied, so "over" is one multiply-add per channel.
    let keep = 1.0 - f32::from(top.a()) / 255.0;
    let mix = |t: u8, u: u8| (f32::from(t) + f32::from(u) * keep).round().min(255.0) as u8;
    Color32::from_rgba_premultiplied(
        mix(top.r(), under.r()),
        mix(top.g(), under.g()),
        mix(top.b(), under.b()),
        mix(top.a(), under.a()),
    )
}

/// The tokens of both themes, as the egui `Context` holds them.
#[derive(Clone)]
struct Installed {
    dark: Arc<Tokens>,
    light: Arc<Tokens>,
}

impl Installed {
    fn new(settings: &Settings) -> Installed {
        Installed {
            dark: Arc::new(Tokens::new(Theme::Dark, settings)),
            light: Arc::new(Tokens::new(Theme::Light, settings)),
        }
    }

    fn of(&self, theme: Theme) -> Arc<Tokens> {
        match theme {
            Theme::Dark => Arc::clone(&self.dark),
            Theme::Light => Arc::clone(&self.light),
        }
    }
}

fn installed_id() -> egui::Id {
    egui::Id::NULL.with("reda-theme-tokens")
}

/// Reading the tokens: `ui.tokens()`, then plain fields such as `t.color.surface.card`,
/// `t.space.gap` and `t.radius.card`.
pub trait TokensExt {
    /// The tokens of the theme being drawn. Before [`install`] they are the defaults, with
    /// text in egui's own fonts.
    fn tokens(&self) -> Arc<Tokens>;
}

impl TokensExt for egui::Context {
    fn tokens(&self) -> Arc<Tokens> {
        let theme = self.theme();
        if let Some(installed) = self.data(|d| d.get_temp::<Installed>(installed_id())) {
            return installed.of(theme);
        }
        static DEFAULT: OnceLock<Installed> = OnceLock::new();
        // No REDA fonts either, so the text styles name egui's own families.
        DEFAULT
            .get_or_init(|| {
                let tokens = |theme| {
                    let settings = Settings::default();
                    Arc::new(Tokens::with_families(theme, &settings, &Families::egui()))
                };
                Installed {
                    dark: tokens(Theme::Dark),
                    light: tokens(Theme::Light),
                }
            })
            .of(theme)
    }
}

impl TokensExt for egui::Ui {
    fn tokens(&self) -> Arc<Tokens> {
        self.ctx().tokens()
    }
}

/// Rebuilds the tokens of both themes from `settings` and generates egui's styles from
/// them. Called again whenever Density, the text size or a font changes.
///
/// The chosen font families are resolved by name against the context's installed fonts
/// ([`fonts::set_source`]); [`fonts::report`] tells what became of them. The first call
/// on a context must come before the pass lays out text ([`fonts::apply`]).
pub fn set(ctx: &egui::Context, settings: &Settings) {
    fonts::apply(ctx, &settings.fonts);
    let installed = Installed::new(settings);
    ctx.set_style_of(Theme::Dark, style(&installed.dark));
    ctx.set_style_of(Theme::Light, style(&installed.light));
    ctx.data_mut(|d| d.insert_temp(installed_id(), installed));
}

/// The egui theme preference that the *General → Theme* Preference asks for.
pub fn preference(prefs: &Preferences) -> egui::ThemePreference {
    match prefs.general.theme {
        preferences::Theme::FollowOs => egui::ThemePreference::System,
        preferences::Theme::Dark => egui::ThemePreference::Dark,
        preferences::Theme::Light => egui::ThemePreference::Light,
    }
}

/// Installs the bundled fonts, the tokens and both themes' styles on `ctx`, in Comfortable
/// at the default text size, and picks light or dark by `preference`.
pub fn install(ctx: &egui::Context, preference: egui::ThemePreference) {
    set(ctx, &Settings::default());
    ctx.set_theme(preference);
}

/// The canvas colours that go with an egui theme.
pub fn canvas_theme(theme: Theme) -> reda_engine::render::Theme {
    match theme {
        Theme::Dark => reda_engine::render::Theme::Dark,
        Theme::Light => reda_engine::render::Theme::Light,
    }
}
