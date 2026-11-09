//! The Checks Preferences that set how Violations show on the canvas: the overlay's
//! style, what it does zoomed out, and the per-check Details and Overlay grid
//! (docs/spec/v0.1/checks.md#canvas-markers).

use std::collections::BTreeMap;

use reda_model::{DrcCheck, ErcCheck};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// How an overlay colours the whole offending object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverlayStyle {
    /// The object's own Layer colour, brightened, with a severity outline.
    LayerColor,
    /// A solid fill in the severity colour.
    Solid,
    /// Diagonal stripes in the severity colour.
    WarningPattern,
    /// Cross-hatching in the severity colour.
    #[default]
    CrossPattern,
}

/// What an overlay pattern does on an object too small on screen to show it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverlayZoomedOut {
    /// The pattern shrinks with the object, so it stays a pattern.
    PatternScales,
    /// The object is filled with its Layer colour.
    LayerColorDominates,
    /// The object is filled with the severity colour.
    #[default]
    OverrideColorDominates,
}

/// How one check's Violations are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CheckDisplay {
    /// Draw the detail marker: a ring, and zoomed in the value and the closest points.
    pub details: bool,
    /// Colour the offending objects.
    pub overlay: bool,
}

impl Default for CheckDisplay {
    fn default() -> CheckDisplay {
        CheckDisplay {
            details: true,
            overlay: false,
        }
    }
}

/// The checks whose overlay is on by default.
pub const OVERLAY_BY_DEFAULT: &[&str] = &["clearance", "track_width", "courtyard_overlap"];

/// The Details and Overlay grid: one [`CheckDisplay`] per ERC and DRC check id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckDisplays(pub BTreeMap<String, CheckDisplay>);

impl CheckDisplays {
    /// Every check id the grid has a row for: ERC's, then DRC's, each once.
    pub fn checks() -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        let ids = ErcCheck::ALL
            .iter()
            .map(|c| c.id())
            .chain(DrcCheck::ALL.iter().map(|c| c.id()));
        for id in ids {
            if !out.contains(&id) {
                out.push(id);
            }
        }
        out
    }

    /// How `check` is drawn; a check the grid does not know gets the default.
    pub fn get(&self, check: &str) -> CheckDisplay {
        self.0.get(check).copied().unwrap_or_default()
    }
}

impl Default for CheckDisplays {
    fn default() -> CheckDisplays {
        CheckDisplays(
            CheckDisplays::checks()
                .into_iter()
                .map(|id| {
                    let display = CheckDisplay {
                        details: true,
                        overlay: OVERLAY_BY_DEFAULT.contains(&id),
                    };
                    (id.to_owned(), display)
                })
                .collect(),
        )
    }
}

impl JsonSchema for CheckDisplays {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CheckDisplays".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let row = serde_json::to_value(generator.subschema_for::<CheckDisplay>())
            .expect("schemas are JSON");
        let properties: Map<String, Value> = CheckDisplays::checks()
            .into_iter()
            .map(|id| (id.to_owned(), row.clone()))
            .collect();
        let value = json!({
            "description": "Per check id: whether its Violations get a detail marker \
                            and an overlay.",
            "type": "object",
            "properties": properties,
            "additionalProperties": false,
        });
        schemars::Schema::try_from(value).expect("an object schema")
    }
}
