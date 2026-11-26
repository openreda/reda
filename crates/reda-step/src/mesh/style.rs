//! Colours: what each styled item (a face, a shell, a solid) is drawn in, read through the
//! style chain `STYLED_ITEM → PRESENTATION_STYLE_ASSIGNMENT → SURFACE_STYLE_USAGE →
//! SURFACE_SIDE_STYLE → SURFACE_STYLE_FILL_AREA → FILL_AREA_STYLE → FILL_AREA_STYLE_COLOUR →
//! COLOUR_RGB | DRAUGHTING_PRE_DEFINED_COLOUR`, with a transparency from
//! `SURFACE_STYLE_RENDERING_WITH_PROPERTIES`.

use std::collections::HashMap;

use super::Colour;
use crate::StepModel;
use crate::records::*;

/// The colour of every styled item. An overriding style wins over the one it overrides.
/// Styles that hold only in one assembly context are not read.
pub(crate) fn colours(model: &StepModel) -> HashMap<RecordId, Colour> {
    let mut out = HashMap::new();
    for (_, s) in model.iter::<StyledItem>() {
        if let Some(c) = style_colour(model, &s.styles) {
            out.insert(s.item, c);
        }
    }
    for (_, s) in model.iter::<OverRidingStyledItem>() {
        if let Some(c) = style_colour(model, &s.styles) {
            out.insert(s.item, c);
        }
    }
    out
}

fn style_colour(model: &StepModel, styles: &[RecordId]) -> Option<Colour> {
    let mut colour = None;
    let mut transparency = None;
    for &psa in styles {
        let Some(psa) = model.get::<PresentationStyleAssignment>(psa) else {
            continue;
        };
        for &usage in &psa.styles {
            let side = match model.get::<SurfaceStyleUsage>(usage) {
                Some(u) => u.style,
                None => usage,
            };
            let Some(side) = model.get::<SurfaceSideStyle>(side) else {
                continue;
            };
            for &element in &side.styles {
                if let Some(fill) = model.get::<SurfaceStyleFillArea>(element)
                    && let Some(fas) = model.get::<FillAreaStyle>(fill.fill_area)
                {
                    for &fasc in &fas.fill_styles {
                        if let Some(c) = model.get::<FillAreaStyleColour>(fasc) {
                            colour = colour.or_else(|| rgb(model, c.fill_colour));
                        }
                    }
                }
                if let Some(r) = model.get::<SurfaceStyleRenderingWithProperties>(element) {
                    colour = colour.or_else(|| rgb(model, r.surface_colour));
                    for &p in &r.properties {
                        if let Some(t) = model.get::<SurfaceStyleTransparent>(p) {
                            transparency = Some(t.transparency);
                        }
                    }
                }
                if let Some(r) = model.get::<SurfaceStyleRendering>(element) {
                    colour = colour.or_else(|| rgb(model, r.surface_colour));
                }
            }
        }
    }
    let mut c = colour?;
    if let Some(t) = transparency {
        c.alpha = (1.0 - t).clamp(0.0, 1.0) as f32;
    }
    Some(c)
}

fn rgb(model: &StepModel, id: RecordId) -> Option<Colour> {
    if let Some(c) = model.get::<ColourRgb>(id) {
        let ch = |x: f64| x.clamp(0.0, 1.0) as f32;
        return Some(Colour::rgb(ch(c.red), ch(c.green), ch(c.blue)));
    }
    let named = model.get::<DraughtingPreDefinedColour>(id)?;
    let (r, g, b) = match named.name.to_ascii_lowercase().as_str() {
        "red" => (1.0, 0.0, 0.0),
        "green" => (0.0, 1.0, 0.0),
        "blue" => (0.0, 0.0, 1.0),
        "yellow" => (1.0, 1.0, 0.0),
        "magenta" => (1.0, 0.0, 1.0),
        "cyan" => (0.0, 1.0, 1.0),
        "black" => (0.0, 0.0, 0.0),
        "white" => (1.0, 1.0, 1.0),
        _ => return None,
    };
    Some(Colour::rgb(r, g, b))
}
