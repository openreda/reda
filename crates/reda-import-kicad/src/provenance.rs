//! Where imported files came from, and the notice about the services' terms.
//!
//! The service is recognised from what the files say about themselves: property names and
//! values, texts, descriptions and model paths. The `generator` token is never looked at,
//! because services write KiCad's own generator names into their files.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sexpr::Sexp;

/// Shown once, the first time a user imports a part.
pub const VENDOR_TERMS_NOTICE: &str = "Parts from vendor download services stay under \
    that service's terms of use, which REDA records with each imported element. Most \
    services do not allow redistributing their files, and SnapMagic allows at most 10 \
    files per location. Check the terms before committing this Library to a public \
    repository.";

/// A part download service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Service {
    /// SnapMagic, formerly SnapEDA.
    Snapmagic,
    UltraLibrarian,
    /// SamacSys, the Component Search Engine.
    Samacsys,
    /// EasyEDA / LCSC parts, converted to KiCad by a third-party tool.
    Easyeda,
    /// Not recognised.
    Unknown,
}

impl Service {
    /// The name recorded in the elements' provenance.
    pub fn name(self) -> &'static str {
        match self {
            Service::Snapmagic => "SnapMagic",
            Service::UltraLibrarian => "Ultra Librarian",
            Service::Samacsys => "SamacSys",
            Service::Easyeda => "EasyEDA",
            Service::Unknown => "Unknown",
        }
    }

    /// The service's terms of use; empty when unknown.
    pub fn terms_url(self) -> &'static str {
        match self {
            Service::Snapmagic => "https://www.snapeda.com/about/terms/",
            Service::UltraLibrarian => "https://www.ultralibrarian.com/legal",
            Service::Samacsys => "https://intelligence.supplyframe.com/terms/",
            Service::Easyeda => "https://easyeda.com/legal",
            Service::Unknown => "",
        }
    }
}

/// Whether `text` is a copyright line or a copyright property name.
pub fn is_copyright(text: &str) -> bool {
    text.to_lowercase().contains("copyright") || text.contains('©')
}

/// Every atom and string in `node`, lower-cased and space-separated, except what is under
/// a `generator` list.
fn words(node: &Sexp, out: &mut String) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n {
            Sexp::List(items, _) => {
                if n.head() != Some("generator") {
                    stack.extend(items.iter());
                }
            }
            Sexp::Atom(s, _) | Sexp::Str(s, _) => {
                out.push(' ');
                out.push_str(&s.to_lowercase());
            }
        }
    }
}

/// The service the pair most likely came from.
pub fn recognise(symbol_lib: &Sexp, footprint: &Sexp) -> Service {
    let mut text = String::new();
    words(symbol_lib, &mut text);
    words(footprint, &mut text);
    let squashed: String = text.chars().filter(|c| c.is_alphanumeric()).collect();
    let has = |needle: &str| squashed.contains(needle);
    if has("snapeda") || has("snapmagic") {
        Service::Snapmagic
    } else if has("samacsys") || has("componentsearchengine") {
        Service::Samacsys
    } else if has("ultralibrarian") || has("accelerateddesigns") {
        Service::UltraLibrarian
    } else if has("easyeda") || has("lcscpart") {
        Service::Easyeda
    } else {
        Service::Unknown
    }
}
