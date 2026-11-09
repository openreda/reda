//! Free text style and Dimension rows of the Properties panel
//! (docs/spec/v0.1/pcb.md#text-style, pcb.md#dimensions). Height, thickness and the
//! Dimension's lengths are numeric rows ([`super::Numbers`]); these are the rest: the
//! font, justification, mirroring, knockout and frame of free text, and a Dimension's
//! units, precision, format, prefix, suffix and text and arrow positions.

use reda_engine::model::geom::Nm;
use reda_engine::model::{
    ArrowPosition, Dimension, DimensionFormat, DimensionTextPosition, DimensionUnits, PcbText,
    Project, TextFrame, TextHJustify, TextJustify, TextVJustify, Uuid,
};
use serde_json::{Value, json};

use super::Shared;
use crate::selection::Selection;

/// The style rows of the selected free text.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyleRows {
    /// The selected texts, with the justification each has now.
    texts: Vec<(Uuid, TextJustify)>,
    pub font: Shared<String>,
    pub h: Shared<TextHJustify>,
    pub v: Shared<TextVJustify>,
    pub mirrored: Shared<bool>,
    /// The knockout margin; `None` without a knockout.
    pub knockout: Shared<Option<Nm>>,
    pub frame: Shared<Option<TextFrame>>,
}

/// One edit of the text style rows.
#[derive(Clone, Debug, PartialEq)]
pub enum TextStyleEdit {
    Font(String),
    H(TextHJustify),
    V(TextVJustify),
    Mirrored(bool),
    /// A knockout with this margin, or none.
    Knockout(Option<Nm>),
    /// A frame, or none.
    Frame(Option<TextFrame>),
}

/// The margin a knockout starts with when it is switched on.
pub const KNOCKOUT_MARGIN: Nm = Nm::from_um(200);

impl TextStyleRows {
    /// The rows for `selection`; `None` unless it has free text.
    pub fn of(project: &Project, selection: &Selection) -> Option<TextStyleRows> {
        let pcb = project.pcb()?;
        let texts: Vec<&PcbText> = selection
            .objects
            .iter()
            .filter_map(|o| pcb.text(*o))
            .collect();
        if texts.is_empty() {
            return None;
        }
        Some(TextStyleRows {
            texts: texts.iter().map(|t| (t.uuid, t.style.justify)).collect(),
            font: Shared::of(texts.iter().map(|t| t.style.font.clone()))?,
            h: Shared::of(texts.iter().map(|t| t.style.justify.h))?,
            v: Shared::of(texts.iter().map(|t| t.style.justify.v))?,
            mirrored: Shared::of(texts.iter().map(|t| t.mirrored))?,
            knockout: Shared::of(texts.iter().map(|t| t.style.knockout.map(|k| k.margin)))?,
            frame: Shared::of(texts.iter().map(|t| t.frame))?,
        })
    }

    /// The `pcb.text.set` per text that makes `edit`, as one Command (a `batch` for
    /// several texts).
    pub fn command(&self, edit: &TextStyleEdit) -> (&'static str, Value) {
        let steps: Vec<Value> = self
            .texts
            .iter()
            .map(|(uuid, justify)| {
                let mut p = json!({ "uuid": uuid });
                match edit {
                    TextStyleEdit::Font(f) => p["font"] = json!(f),
                    TextStyleEdit::H(h) => p["justify"] = json!({"h": h, "v": justify.v}),
                    TextStyleEdit::V(v) => p["justify"] = json!({"h": justify.h, "v": v}),
                    TextStyleEdit::Mirrored(m) => p["mirrored"] = json!(m),
                    TextStyleEdit::Knockout(Some(margin)) => {
                        p["knockout"] = json!({"margin": margin.0});
                    }
                    TextStyleEdit::Knockout(None) => p["clear_knockout"] = json!(true),
                    TextStyleEdit::Frame(Some(f)) => {
                        p["frame"] = json!({"width": f.width.0, "height": f.height.0});
                    }
                    TextStyleEdit::Frame(None) => p["clear_frame"] = json!(true),
                }
                p
            })
            .collect();
        one_or_batch("pcb.text.set", steps)
    }
}

/// `id` with each of `params`: alone for one, else a `batch`.
fn one_or_batch(id: &'static str, mut params: Vec<Value>) -> (&'static str, Value) {
    if params.len() == 1 {
        return (id, params.remove(0));
    }
    let commands: Vec<Value> = params
        .into_iter()
        .map(|p| json!({"id": id, "params": p}))
        .collect();
    ("batch", json!({ "commands": commands }))
}

/// The rows of the selected Dimensions.
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionRows {
    dimensions: Vec<Uuid>,
    /// The value each shows, as its text reads ("12.70mm").
    pub labels: Vec<String>,
    pub units: Shared<DimensionUnits>,
    pub precision: Shared<u8>,
    pub format: Shared<DimensionFormat>,
    pub prefix: Shared<String>,
    pub suffix: Shared<String>,
    pub text_position: Shared<DimensionTextPosition>,
    pub arrow_position: Shared<ArrowPosition>,
}

/// One edit of the Dimension rows.
#[derive(Clone, Debug, PartialEq)]
pub enum DimensionEdit {
    Units(DimensionUnits),
    Precision(u8),
    Format(DimensionFormat),
    Prefix(String),
    Suffix(String),
    TextPosition(DimensionTextPosition),
    ArrowPosition(ArrowPosition),
}

impl DimensionRows {
    /// The rows for `selection`; `None` unless it has Dimensions.
    pub fn of(project: &Project, selection: &Selection) -> Option<DimensionRows> {
        let pcb = project.pcb()?;
        let dims: Vec<&Dimension> = selection
            .objects
            .iter()
            .filter_map(|o| pcb.dimension(*o))
            .collect();
        if dims.is_empty() {
            return None;
        }
        Some(DimensionRows {
            dimensions: dims.iter().map(|d| d.uuid).collect(),
            labels: dims
                .iter()
                .map(|d| reda_engine::pcb::dimension::label(d))
                .collect(),
            units: Shared::of(dims.iter().map(|d| d.units))?,
            precision: Shared::of(dims.iter().map(|d| d.precision))?,
            format: Shared::of(dims.iter().map(|d| d.format))?,
            prefix: Shared::of(dims.iter().map(|d| d.prefix.clone()))?,
            suffix: Shared::of(dims.iter().map(|d| d.suffix.clone()))?,
            text_position: Shared::of(dims.iter().map(|d| d.text_position))?,
            arrow_position: Shared::of(dims.iter().map(|d| d.arrow_position))?,
        })
    }

    /// The `pcb.dimension.set` per Dimension that makes `edit`, as one Command.
    pub fn command(&self, edit: &DimensionEdit) -> (&'static str, Value) {
        let (name, value) = match edit {
            DimensionEdit::Units(u) => ("units", json!(u)),
            DimensionEdit::Precision(p) => ("precision", json!(p)),
            DimensionEdit::Format(f) => ("format", json!(f)),
            DimensionEdit::Prefix(s) => ("prefix", json!(s)),
            DimensionEdit::Suffix(s) => ("suffix", json!(s)),
            DimensionEdit::TextPosition(t) => ("text_position", json!(t)),
            DimensionEdit::ArrowPosition(a) => ("arrow_position", json!(a)),
        };
        let steps = self
            .dimensions
            .iter()
            .map(|uuid| json!({ "uuid": uuid, name: value }))
            .collect();
        one_or_batch("pcb.dimension.set", steps)
    }
}
