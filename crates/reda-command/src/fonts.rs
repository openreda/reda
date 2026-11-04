//! The font bytes a front end hands to the engine.
//!
//! The engine reads no font files itself: the app finds the fonts installed on the machine
//! and adds their bytes here, by family name. Text whose family is not here (or whose bytes
//! do not load) draws in a fallback font and is reported as missing.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// One font face: its bytes, and the face index within a collection (TTC/OTC).
#[derive(Clone)]
pub struct FontFace {
    /// The family name as it was added.
    pub family: String,
    pub bytes: Arc<[u8]>,
    pub index: u32,
}

impl fmt::Debug for FontFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FontFace")
            .field("family", &self.family)
            .field("bytes", &self.bytes.len())
            .field("index", &self.index)
            .finish()
    }
}

/// Font faces by family name, matched case-insensitively. Cheap to clone.
#[derive(Clone, Debug, Default)]
pub struct Fonts {
    faces: BTreeMap<String, FontFace>,
}

impl Fonts {
    /// Adds (or replaces) a single-font file's bytes under `family`.
    pub fn add(&mut self, family: impl Into<String>, bytes: impl Into<Arc<[u8]>>) {
        self.add_face(family, bytes, 0);
    }

    /// Adds (or replaces) face `index` of a font collection under `family`.
    pub fn add_face(&mut self, family: impl Into<String>, bytes: impl Into<Arc<[u8]>>, index: u32) {
        let family = family.into();
        let face = FontFace {
            family: family.clone(),
            bytes: bytes.into(),
            index,
        };
        self.faces.insert(family.to_lowercase(), face);
    }

    /// The face added under this family, compared case-insensitively.
    pub fn get(&self, family: &str) -> Option<&FontFace> {
        self.faces.get(&family.to_lowercase())
    }

    /// Every family, as added, sorted case-insensitively.
    pub fn families(&self) -> impl Iterator<Item = &str> {
        self.faces.values().map(|f| f.family.as_str())
    }
}
