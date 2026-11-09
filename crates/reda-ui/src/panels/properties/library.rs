//! The Library rows of a selected Component: its Library Status badge with Update…, and
//! the Part's name with Replace… and Edit in Library.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{RichText, Role, Ui, WidgetInfo};
use reda_engine::model::Uuid;
use reda_engine::schematic::{Aspect, ComponentStatus, LibraryStatus};
use serde_json::json;

use super::Out;
use crate::properties::{ComponentProperties, MIXED, Shared};
use crate::schematic_dialogs::update::status_text;
use crate::shell::Shell;
use crate::tr;

/// Every placed Component's Library Status, and the library generation and Journal length
/// it was worked out at, kept between frames.
#[derive(Clone, Default)]
struct Cached(Option<((u64, usize), Arc<BTreeMap<Uuid, ComponentStatus>>)>);

/// The Library Statuses of the Shell's Project, worked out again only after a Command ran
/// or the Libraries changed.
fn statuses(ui: &Ui, shell: &Shell) -> Arc<BTreeMap<Uuid, ComponentStatus>> {
    let key = (shell.library_generation(), shell.engine().journal().len());
    let id = egui::Id::NULL.with("properties-library-statuses");
    let cached: Cached = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    if let Some((k, statuses)) = cached.0
        && k == key
    {
        return statuses;
    }
    let statuses: BTreeMap<Uuid, ComponentStatus> = match shell.engine().project() {
        Some(project) => {
            let index = shell.engine().libraries();
            reda_engine::library::library_statuses(project, &index)
                .into_iter()
                .map(|s| (s.component, s))
                .collect()
        }
        None => BTreeMap::new(),
    };
    let statuses = Arc::new(statuses);
    ui.ctx()
        .data_mut(|d| d.insert_temp(id, Cached(Some((key, statuses.clone())))));
    statuses
}

/// The status the Components share, with the aspects that differ across them.
pub fn shared_status(
    statuses: &BTreeMap<Uuid, ComponentStatus>,
    components: &[Uuid],
) -> Option<Shared<(LibraryStatus, Vec<Aspect>)>> {
    Shared::of(components.iter().filter_map(|c| {
        let s = statuses.get(c)?;
        Some((s.status, s.aspects.clone().unwrap_or_default()))
    }))
}

/// Draws the Library Status and Part rows above the Component grid.
pub(super) fn rows(
    ui: &mut Ui,
    shell: &Shell,
    component: &ComponentProperties,
    out: &mut Vec<Out>,
) {
    let Some(part) = &component.part else {
        return;
    };
    let objects = &shell.selection().objects;
    let statuses = statuses(ui, shell);
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("properties-library-status"));
        let (text, warn) = match shared_status(&statuses, &component.components) {
            Some(Shared::Same((status, aspects))) => (
                status_text(status, &aspects),
                status != LibraryStatus::UpToDate,
            ),
            Some(Shared::Mixed) => (MIXED.to_owned(), true),
            None => (String::new(), false),
        };
        let color = if warn {
            ui.visuals().warn_fg_color
        } else {
            ui.visuals().text_color()
        };
        let badge = ui.label(RichText::new(&text).color(color).strong());
        let name = tr!("properties-library-status-badge", status = text.clone());
        badge.widget_info(|| WidgetInfo::labeled(Role::Label, true, &name));
        if ui
            .button(tr!("properties-update"))
            .on_hover_text(tr!("properties-update-hint"))
            .clicked()
        {
            out.push(Out::Run(
                "schematic.update_from_library.open",
                json!({ "objects": objects }),
            ));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(tr!("properties-part"));
        ui.label(match part {
            Shared::Same((_, name)) => name.clone(),
            Shared::Mixed => MIXED.to_owned(),
        });
        if ui
            .button(tr!("properties-replace"))
            .on_hover_text(tr!("properties-replace-hint"))
            .clicked()
        {
            out.push(Out::Run(
                "schematic.component.replace.open",
                json!({ "objects": objects }),
            ));
        }
        if ui.button(tr!("properties-edit-in-library")).clicked() {
            out.push(Out::Run(
                "library.element.edit_in_library",
                json!({ "objects": objects }),
            ));
        }
    });
}
