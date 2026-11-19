//! Physical materials in their own colours, shared with the Layers table.
use crate::settings::strong;
use crate::theme::TokensExt as _;
use crate::{tr, widgets};
use egui::{Color32, Rect, Role, Sense, Ui, WidgetInfo, pos2, vec2};
use reda_engine::model::{DielectricKind, PcbDocument, StackupEntry, StackupInfo};

pub(super) fn color(entry: &StackupEntry) -> Color32 {
    match entry {
        StackupEntry::Copper(_) => widgets::data_color(0xc8, 0x75, 0x33),
        StackupEntry::Dielectric(d) if d.kind == DielectricKind::Core => {
            widgets::data_color(0xb5, 0xa6, 0x5a)
        }
        StackupEntry::Dielectric(_) => widgets::data_color(0xd3, 0xc7, 0x8a),
    }
}

fn mask_color(name: Option<&str>) -> Color32 {
    match name.unwrap_or("").to_lowercase().as_str() {
        n if n.contains("red") => widgets::data_color(0xb7, 0x1c, 0x1c),
        n if n.contains("blue") => widgets::data_color(0x15, 0x65, 0xc0),
        n if n.contains("black") => widgets::data_color(0x21, 0x21, 0x21),
        n if n.contains("white") => widgets::data_color(0xf5, 0xf5, 0xf5),
        n if n.contains("yellow") => widgets::data_color(0xf9, 0xa8, 0x25),
        n if n.contains("purple") => widgets::data_color(0x6a, 0x1b, 0x9a),
        _ => widgets::data_color(0x2e, 0x7d, 0x32),
    }
}

/// A resolved thickness in mm, including a fractional share of a nanometre.
pub(super) fn mm(nm: f64) -> String {
    format!("{:.9}", nm / 1_000_000.0)
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub(super) fn show(ui: &mut Ui, pcb: &PcbDocument, info: &StackupInfo) {
    let t = ui.tokens();
    strong(ui, tr!("board-setup-cross-section"));
    let stack = pcb.stackup();
    crate::settings::layout::muted(
        ui,
        tr!(
            "stackup-section-total",
            thickness = info.totals.total.mm_exact()
        ),
    );
    let height = ui.available_height().clamp(1.0, t.size.of(360.0));
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let draw = ui.painter_at(rect);
    let mask = mask_color(stack.mask_color.as_deref());
    let mask_height = t.stroke.accent_line.min(rect.height() / 2.0);
    let mut y = rect.top() + mask_height;
    draw.rect_filled(rect.with_max_y(y), 0.0, mask);
    let body = (rect.height() - 2.0 * mask_height).max(0.0);
    let mut labels = Vec::new();
    for (i, (entry, resolved)) in stack.entries.iter().zip(&info.entries).enumerate() {
        let h = (resolved.thickness / info.totals.total.0 as f64) as f32 * body;
        let band = Rect::from_min_max(pos2(rect.left(), y), pos2(rect.right(), y + h));
        draw.rect_filled(band, 0.0, color(entry));
        labels.push(format!(
            "{} {} mm",
            super::entry_name(pcb, i),
            mm(resolved.thickness)
        ));
        y += h;
    }
    draw.rect_filled(rect.with_min_y(y), 0.0, mask);
    let summary = tr!("board-setup-cross-section-of", bands = labels.join(", "));
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &summary));
}
