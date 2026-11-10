//! The grouped Active Bar (the Preference `display.group_active_bar_tools`): each group's
//! button shows its face, the last-used Tool in the group on that Document type, and runs
//! it; a right-click lists the group's Tools (docs/spec/v0.1/app.md#window-shell).

use egui::{Id, Ui};

use super::draw::{Cx, Invocation};
use super::{Action, DocumentType, Item, Menu, resolve_for};

/// The Tool a group item activates, from its params: `power_port` for every Power Port
/// Preset.
fn tool_of(cx: &Cx<'_>, action: &Action) -> Option<String> {
    let resolved = resolve_for(cx.shell, action, cx.document);
    resolved.params.get("tool")?.as_str().map(str::to_owned)
}

/// The group's Tools, in order.
fn members(group: &Menu) -> impl Iterator<Item = &Action> {
    group.items.iter().filter_map(|item| match item {
        Item::Action(action) => Some(action),
        _ => None,
    })
}

/// The face of `group`: the item `remembered` names (the last one used), else the first;
/// unless the `active` Tool is another of the group's Tools, which then becomes the face,
/// so a key or a menu choosing a Tool shows on its group too. `tool` names the Tool an
/// item activates.
pub fn face<'a>(
    group: &'a Menu,
    remembered: Option<&str>,
    active: Option<&str>,
    tool: impl Fn(&Action) -> Option<String>,
) -> Option<&'a Action> {
    let current = members(group)
        .find(|a| Some(a.id.as_str()) == remembered)
        .or_else(|| members(group).next())?;
    let Some(active) = active else {
        return Some(current);
    };
    if tool(current).as_deref() == Some(active) {
        return Some(current);
    }
    Some(
        members(group)
            .find(|a| tool(a).as_deref() == Some(active))
            .unwrap_or(current),
    )
}

/// Where the face of `group` is remembered for Document type `ty`, for the session.
fn face_id(ty: DocumentType, group: &Menu) -> Id {
    Id::NULL.with(("active-bar-face", format!("{ty:?}"), group.id.as_str()))
}

/// The button of `group` on the Active Bar of a `ty` canvas: its face's icon, which runs
/// the face's Tool, with a painted corner mark and the group's Tools on a right-click.
pub(super) fn button(
    ui: &mut Ui,
    cx: &Cx<'_>,
    ty: DocumentType,
    group: &Menu,
) -> Option<Invocation> {
    let key = face_id(ty, group);
    let remembered: Option<String> = ui.data(|d| d.get_temp(key));
    // Only the active canvas follows the active Tool; another Document type's canvas
    // keeps its own faces.
    let active_canvas =
        cx.document.is_some() && cx.shell.active_document().map(|d| d.document) == cx.document;
    let active = cx
        .checked
        .get("tool.activate")
        .and_then(|p| p.get("tool"))
        .and_then(|t| t.as_str())
        .filter(|_| active_canvas);
    let face = face(group, remembered.as_deref(), active, |a| tool_of(cx, a))?;
    if remembered.as_deref() != Some(face.id.as_str()) {
        ui.data_mut(|d| d.insert_temp(key, face.id.clone()));
    }
    let (mut clicked, response) = super::draw::icon_button_response(ui, cx, face);
    // The corner: a small triangle at the bottom right, painted (the UI font lacks the
    // glyph), which opens the group's menu, as a right-click on the face does.
    let r = response.rect;
    let corner =
        egui::Rect::from_min_max(r.right_bottom() - egui::vec2(12.0, 12.0), r.right_bottom());
    let title = crate::i18n::translate(&group.title, &[]);
    let name = crate::tr!("active-bar-group", group = title.clone());
    let opener = ui.interact(corner, response.id.with("group-menu"), egui::Sense::click());
    let color = if opener.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            r.right_bottom() + egui::vec2(-3.0, -3.0),
            r.right_bottom() + egui::vec2(-3.0, -9.0),
            r.right_bottom() + egui::vec2(-9.0, -3.0),
        ],
        color,
        egui::Stroke::NONE,
    ));
    opener.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    let opener = opener.on_hover_text(&name);
    let open = if response.secondary_clicked() {
        Some(egui::SetOpenCommand::Bool(true))
    } else {
        None
    };
    egui::Popup::menu(&opener)
        .open_memory(open.or(opener.clicked().then_some(egui::SetOpenCommand::Toggle)))
        .accessible_name(title)
        .show(|ui| {
            let mut chosen = None;
            for action in members(group) {
                let mut inner = None;
                super::draw::menu_items(
                    ui,
                    cx,
                    std::slice::from_ref(&Item::Action(action.clone())),
                    &mut inner,
                );
                if inner.is_some() {
                    chosen = Some(action.id.clone());
                    clicked = inner;
                }
            }
            if let Some(id) = chosen {
                ui.data_mut(|d| d.insert_temp(key, id));
            }
        });
    clicked
}
