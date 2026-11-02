//! The UI string catalogue.
//!
//! Every user-facing string in the UI goes through [`tr!`](crate::tr) with a key in the
//! English Fluent catalogue (`i18n/en-US.ftl`). v0.1 ships English only; a test fails when
//! a key the UI uses is missing from the catalogue. Only the UI translates: Command ids,
//! the Journal, files and error results stay English.

use std::sync::OnceLock;

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource, FluentValue};

const EN_US: &str = include_str!("../i18n/en-US.ftl");

fn bundle() -> &'static FluentBundle<FluentResource> {
    static BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let resource = FluentResource::try_new(EN_US.to_owned())
            .unwrap_or_else(|(_, errors)| panic!("i18n/en-US.ftl does not parse: {errors:?}"));
        let locale: unic_langid::LanguageIdentifier = "en-US".parse().expect("valid locale");
        let mut bundle = FluentBundle::new_concurrent(vec![locale]);
        // The UI is not mixing scripts, so the bidi isolation marks would only show as junk.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .unwrap_or_else(|errors| panic!("i18n/en-US.ftl repeats keys: {errors:?}"));
        bundle
    })
}

/// Whether the catalogue has a message for `key`.
pub fn has_key(key: &str) -> bool {
    bundle().get_message(key).and_then(|m| m.value()).is_some()
}

/// The text for `key` with `args` filled in; the key itself when the catalogue lacks it.
/// Use [`tr!`](crate::tr) rather than calling this directly, so the key check sees the key.
pub fn translate(key: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    let bundle = bundle();
    let Some(pattern) = bundle.get_message(key).and_then(|m| m.value()) else {
        return key.to_owned();
    };
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(*name, value.clone());
    }
    let mut errors = Vec::new();
    let args = (!args.is_empty()).then_some(&fluent_args);
    bundle
        .format_pattern(pattern, args, &mut errors)
        .into_owned()
}

/// Turns a value into a Fluent argument. Strings stay strings; numbers stay numbers so
/// plural selectors work.
pub fn arg<'a>(value: impl Into<FluentValue<'a>>) -> FluentValue<'a> {
    value.into()
}

/// Looks a UI string up in the catalogue: `tr!("start-page-title")`, or with arguments
/// `tr!("canvas-description", document = name)`.
#[macro_export]
macro_rules! tr {
    ($key:literal) => {
        $crate::i18n::translate($key, &[])
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::translate(
            $key,
            &[$((stringify!($name), $crate::i18n::arg($value))),+],
        )
    };
}
