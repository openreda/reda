//! The stitching Tool's choice and the selected Via's physical mask sides.
use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::{MaskSide, PcbDocument, Side, Via};
use serde_json::json;

use super::{Out, text_field, units};
use crate::properties::Shared;
use crate::shell::Shell;
use crate::{tr, widgets};

pub(super) fn tool_options(ui: &mut Ui, shell: &Shell) -> Option<Vec<Out>> {
    if shell.tool() != crate::tools::Tool::Via || !shell.board_is_active() {
        return None;
    }
    let pcb = shell.engine().project()?.pcb()?;
    let current = shell.board_view().stitching_type(pcb);
    let mut out = Vec::new();
    let label = tr!("via-type");
    ui.label(&label);
    let menu = widgets::dropdown("stitching-via", current.display_name(pcb)).show(ui, |ui| {
        for kind in &pcb.stackup().via_types {
            if ui
                .add(widgets::choice(
                    current.uuid == kind.uuid,
                    kind.display_name(pcb),
                ))
                .clicked()
            {
                out.push(Out::Run("pcb.via.pick_type", json!({"via_type":kind.uuid})));
            }
        }
    });
    menu.response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, &label));
    Some(out)
}

pub(super) fn selected(
    ui: &mut Ui,
    shell: &Shell,
    pcb: &PcbDocument,
    via: &Via,
    out: &mut Vec<Out>,
) {
    let name = pcb
        .stackup()
        .type_for_span(via.span)
        .map_or_else(|| tr!("via-no-type"), |t| t.display_name(pcb));
    let mask = via.mask.unwrap_or_default();
    widgets::form(("via-properties",via.uuid)).show(ui,|ui|{
        widgets::form_row(ui,tr!("via-type"),|ui|ui.label(name));
        diameter_rows(ui, shell, pcb, via, out);
        for (side,label,value) in [(Side::Top,tr!("via-top-mask"),mask.top),(Side::Bottom,tr!("via-bottom-mask"),mask.bottom)] {
            let shown=match value {
                Some(MaskSide::Mode(_))=>"tent".into(),
                Some(MaskSide::Expansion{expansion})=>crate::pcb::view::format_length(expansion,units(shell)),
                None=>String::new(),
            };
            let key=format!("via-mask-{}-{side:?}",via.uuid);
            if let Some(edit)=text_field(ui,&key,&label,&Shared::Same(shown),via.span.reaches(pcb,side)) {
                let mut updated=serde_json::to_value(mask).expect("Via mask serializes");
                let text=edit.trim();
                let field=if side==Side::Top {"top"} else {"bottom"};
                if text.is_empty() {
                    updated.as_object_mut().unwrap().remove(field);
                } else if text.eq_ignore_ascii_case("tent") {
                    updated[field]=json!("tent");
                } else {
                    updated[field]=json!({"expansion":super::parse_length(text,units(shell)).map_or_else(||json!(text),|v|json!(v))});
                }
                out.push(Out::Run("pcb.via.set",json!({"vias":[via.uuid],"mask":updated})));
            }
            ui.end_row();
        }
    });
}

fn diameter_rows(ui: &mut Ui, shell: &Shell, pcb: &PcbDocument, via: &Via, out: &mut Vec<Out>) {
    let units = units(shell);
    let middle = pcb.span_layers(via.span).len() > 2;
    for (field, label, value) in [
        ("start", tr!("via-start-diameter"), Some(via.diameter.start)),
        ("middle", tr!("via-middle-diameter"), via.diameter.middle),
        ("end", tr!("via-end-diameter"), via.diameter.end),
        ("hole", tr!("via-hole-size"), Some(via.hole)),
    ] {
        if field == "middle" && !middle {
            continue;
        }
        let shown = value
            .map(|v| crate::pcb::view::format_length(v, units))
            .unwrap_or_default();
        let key = format!("via-size-{}-{field}", via.uuid);
        if let Some(edit) = text_field(ui, &key, &label, &Shared::Same(shown), !via.locked) {
            let text = edit.trim();
            let mut params = json!({"vias":[via.uuid]});
            if field == "hole" {
                params["hole"] =
                    super::parse_length(text, units).map_or_else(|| json!(text), |v| json!(v));
            } else {
                let mut diameter = json!(via.diameter);
                if text.is_empty() && field != "start" {
                    diameter
                        .as_object_mut()
                        .expect("diameter is an object")
                        .remove(field);
                } else {
                    diameter[field] =
                        super::parse_length(text, units).map_or_else(|| json!(text), |v| json!(v));
                }
                params["diameter"] = diameter;
            }
            out.push(Out::Run("pcb.via.set", params));
        }
        ui.end_row();
    }
}
