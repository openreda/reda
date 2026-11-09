//! The Sheet region's frame controls, with nothing selected on a sheet: paper size (a
//! standard one or a custom width and height), orientation, Border Zones, the title block
//! and the sheet's drawing number and revision. Each change is one
//! `schematic.sheet.set_frame`; the title block's other fields show read-only.

use egui::{Grid, Ui};
use reda_engine::model::{FrameSize, Orientation, Project, SheetSize, Uuid};
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use super::{Out, text_field};
use crate::pcb::view::format_length;
use crate::properties::Shared;
use crate::tr;

const SIZES: [SheetSize; 8] = [
    SheetSize::A4,
    SheetSize::A3,
    SheetSize::A2,
    SheetSize::A1,
    SheetSize::A0,
    SheetSize::Letter,
    SheetSize::Legal,
    SheetSize::Tabloid,
];

fn size_name(size: SheetSize) -> &'static str {
    match size {
        SheetSize::A4 => "A4",
        SheetSize::A3 => "A3",
        SheetSize::A2 => "A2",
        SheetSize::A1 => "A1",
        SheetSize::A0 => "A0",
        SheetSize::Letter => "Letter",
        SheetSize::Legal => "Legal",
        SheetSize::Tabloid => "Tabloid",
    }
}

/// A typed length: as given when it has a unit, else in millimetres.
fn length_text(text: &str) -> String {
    let text = text.trim();
    match text.ends_with(|c: char| c.is_ascii_digit() || c == '.') {
        true => format!("{text}mm"),
        false => text.to_owned(),
    }
}

/// The sheet's name, frame and title block. Frame edits go to `out`.
pub(super) fn show(ui: &mut Ui, project: &Project, document: Uuid, out: &mut Vec<Out>) {
    let name = project
        .documents()
        .find(|d| d.uuid == document)
        .map(|d| d.name.clone())
        .unwrap_or_default();
    let Some(sheet) = project.schematic_sheet(document) else {
        return;
    };
    let frame = sheet.frame_settings();
    let Ok(block) = reda_engine::schematic::title_block(project, document) else {
        return;
    };
    let mut set = |params: Value| {
        let mut params = params;
        params["document"] = json!(document);
        out.push(Out::Run("schematic.sheet.set_frame", params));
    };
    Grid::new("properties-sheet").num_columns(2).show(ui, |ui| {
        ui.weak(tr!("properties-sheet-name"));
        ui.label(name);
        ui.end_row();

        // Paper size: a standard one, or Custom with its width and height.
        ui.weak(tr!("properties-sheet-size"));
        let shown = match frame.size {
            FrameSize::Standard(size) => size_name(size).to_owned(),
            FrameSize::Custom { .. } => tr!("properties-sheet-custom"),
        };
        let label = tr!("properties-sheet-size");
        let combo = egui::ComboBox::from_id_salt("properties-sheet-size")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                for size in SIZES {
                    let chosen = frame.size == FrameSize::Standard(size);
                    if ui.selectable_label(chosen, size_name(size)).clicked() && !chosen {
                        set(json!({"size": size}));
                    }
                }
                let custom = matches!(frame.size, FrameSize::Custom { .. });
                if ui
                    .selectable_label(custom, tr!("properties-sheet-custom"))
                    .clicked()
                    && !custom
                {
                    let (long, short) = frame.size.sides();
                    set(json!({"size": {"width": long.0, "height": short.0}}));
                }
            });
        combo
            .response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &label));
        ui.end_row();
        if let FrameSize::Custom { width, height } = frame.size {
            let mm = |n| Shared::Same(format_length(n, Units::Mm));
            if let Some(text) = text_field(
                ui,
                "sheet-width",
                &tr!("properties-sheet-width"),
                &mm(width),
                true,
            ) {
                set(json!({"size": {"width": length_text(&text), "height": height.0}}));
            }
            ui.end_row();
            if let Some(text) = text_field(
                ui,
                "sheet-height",
                &tr!("properties-sheet-height"),
                &mm(height),
                true,
            ) {
                set(json!({"size": {"width": width.0, "height": length_text(&text)}}));
            }
            ui.end_row();
        }

        ui.weak(tr!("properties-sheet-orientation"));
        ui.vertical(|ui| {
            for (orientation, label) in [
                (Orientation::Landscape, tr!("properties-sheet-landscape")),
                (Orientation::Portrait, tr!("properties-sheet-portrait")),
            ] {
                let chosen = frame.orientation == orientation;
                if ui.radio(chosen, label).clicked() && !chosen {
                    set(json!({ "orientation": orientation }));
                }
            }
        });
        ui.end_row();

        ui.weak(tr!("properties-sheet-frame"));
        ui.vertical(|ui| {
            let mut zones = frame.border_zones;
            if ui
                .checkbox(&mut zones, tr!("properties-sheet-border-zones"))
                .changed()
            {
                set(json!({ "border_zones": zones }));
            }
            let mut title_block = frame.title_block;
            if ui
                .checkbox(&mut title_block, tr!("properties-sheet-title-block"))
                .changed()
            {
                set(json!({ "title_block": title_block }));
            }
        });
        ui.end_row();

        if let Some(text) = text_field(
            ui,
            "sheet-drawing-number",
            &tr!("properties-sheet-drawing-number"),
            &Shared::Same(frame.drawing_number.clone()),
            true,
        ) {
            set(json!({ "drawing_number": text }));
        }
        ui.end_row();
        if let Some(text) = text_field(
            ui,
            "sheet-revision",
            &tr!("properties-sheet-own-revision"),
            &Shared::Same(frame.revision.clone()),
            true,
        ) {
            set(json!({ "revision": text }));
        }
        ui.end_row();
        if frame.revision.is_empty() && !block.revision.is_empty() {
            ui.label("");
            ui.weak(tr!(
                "properties-sheet-project-revision",
                revision = block.revision.clone()
            ));
            ui.end_row();
        }

        let mut row = |name: String, value: String| {
            ui.weak(name);
            ui.label(value);
            ui.end_row();
        };
        row(tr!("properties-sheet-title"), block.title.clone());
        row(tr!("properties-sheet-date"), block.date.clone());
        row(tr!("properties-sheet-author"), block.author.clone());
        row(tr!("properties-sheet-company"), block.company.clone());
        row(
            tr!("properties-sheet-number"),
            format!("{} / {}", block.sheet_number, block.sheet_count),
        );
    });
}
