//! An Output Job's Name Template: the stem of its zip and its BOM and CPL files.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::text::{self, TextError};

/// The tokens a Name Template may use. `${variant}` is reserved for later.
pub const NAME_TEMPLATE_TOKENS: [&str; 5] = ["project", "job", "revision", "date", "preset"];

/// Characters a Name Template may not hold outside its tokens: a path separator, or one
/// that some file systems refuse.
const REFUSED: &[char] = &['<', '>', ':', '"', '\\', '|', '?', '*', '/'];

/// A checked Name Template: only known tokens, and no character a file name can't hold.
/// Commands and file loading both build it through [`NameTemplate::new`], so a stored
/// template is always valid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NameTemplate(String);

/// Why a Name Template is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameTemplateError(String);

impl fmt::Display for NameTemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NameTemplateError {}

impl NameTemplate {
    /// Checks `raw` and wraps it.
    pub fn new(raw: impl Into<String>) -> Result<Self, NameTemplateError> {
        let raw = raw.into();
        if raw.trim().is_empty() {
            return Err(NameTemplateError("the name template is empty".into()));
        }
        let known = |name: &str| NAME_TEMPLATE_TOKENS.contains(&name).then(String::new);
        let literal = text::resolve(&raw, known).map_err(|e| {
            NameTemplateError(match e {
                TextError::Unknown { name, at } if name == "variant" => {
                    format!("\"${{variant}}\" at character {at} is reserved for a later version")
                }
                e => e.to_string(),
            })
        })?;
        if let Some(c) = literal.chars().find(|c| REFUSED.contains(c)) {
            return Err(NameTemplateError(format!("a file name can't hold {c:?}")));
        }
        Ok(Self(raw))
    }

    /// The template with its tokens replaced by `values` (one per [`NAME_TEMPLATE_TOKENS`]
    /// name). A value may still bring in a character a file name can't hold, such as a
    /// `/` in the revision, which is refused here.
    pub fn resolve(
        &self,
        values: impl Fn(&str) -> Option<String>,
    ) -> Result<String, NameTemplateError> {
        let name = text::resolve(&self.0, values).map_err(|e| NameTemplateError(e.to_string()))?;
        if let Some(c) = name.chars().find(|c| REFUSED.contains(c)) {
            return Err(NameTemplateError(format!(
                "the name \"{name}\" can't hold {c:?}"
            )));
        }
        if name.trim().is_empty() {
            return Err(NameTemplateError(
                "the name template resolves to nothing".into(),
            ));
        }
        Ok(name)
    }

    /// The raw template, tokens unresolved.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for NameTemplate {
    /// `${project}-${job}`.
    fn default() -> Self {
        Self("${project}-${job}".into())
    }
}

impl TryFrom<String> for NameTemplate {
    type Error = NameTemplateError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

impl From<NameTemplate> for String {
    fn from(template: NameTemplate) -> Self {
        template.0
    }
}

impl fmt::Display for NameTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_tokens_and_escapes_pass() {
        for good in [
            "${project}-${job}",
            "${project}_${revision}_${date}_${preset}",
            "rev $${not a token}",
        ] {
            assert!(NameTemplate::new(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn unknown_tokens_and_file_name_characters_are_refused() {
        for bad in [
            "",
            "  ",
            "${variant}",
            "${nope}",
            "${project",
            "a:b",
            "a/b",
            "a*b",
        ] {
            assert!(NameTemplate::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn resolving_fills_the_tokens_and_refuses_a_value_a_file_name_cannot_hold() {
        let template =
            NameTemplate::new("${project}_${revision}_${date}_${preset}-${job}").unwrap();
        let values = |revision: &'static str| {
            move |name: &str| {
                Some(match name {
                    "project" => "Demo".to_owned(),
                    "revision" => revision.to_owned(),
                    "date" => "2026-01-01".to_owned(),
                    "preset" => "JLCPCB".to_owned(),
                    "job" => "Bare".to_owned(),
                    _ => return None,
                })
            }
        };
        assert_eq!(
            template.resolve(values("B")).unwrap(),
            "Demo_B_2026-01-01_JLCPCB-Bare"
        );
        assert!(template.resolve(values("A/B")).is_err());
        assert_eq!(
            NameTemplate::default().resolve(values("")).unwrap(),
            "Demo-Bare"
        );
    }

    #[test]
    fn loading_refuses_an_invalid_template() {
        let err = serde_json::from_str::<NameTemplate>("\"${variant}\"").unwrap_err();
        assert!(err.to_string().contains("reserved"), "{err}");
        let ok: NameTemplate = serde_json::from_str("\"${project}\"").unwrap();
        assert_eq!(ok.as_str(), "${project}");
    }
}
