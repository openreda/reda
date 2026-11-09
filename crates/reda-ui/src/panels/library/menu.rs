//! A Library panel row's menu (right-click): Place, Edit, Duplicate, Where Used, Update in
//! Project…, Check, Copy to Library… and Open Datasheet.

use egui::Ui;
use serde_json::json;

use super::rows::{Entry, RowStatus};
use super::{Do, Placing, place};
use crate::tr;

/// The menu of `entry`'s row, placing into `target`; `status` is a Part's Library Status.
pub(super) fn show(
    ui: &mut Ui,
    entry: &Entry,
    target: Placing,
    status: Option<RowStatus>,
) -> Option<Do> {
    let mut picked = None;
    if let Some(part) = &entry.part
        && target != Placing::Nowhere
        && ui.button(tr!("library-menu-place")).clicked()
    {
        picked = place(target, part, None, part.footprints.first().copied()).map(Do::Out);
    }
    if ui.button(tr!("library-menu-edit")).clicked() {
        picked = Some(Do::open(entry.uuid));
    }
    // Duplicate copies it into its own Library, which the starter Library is not.
    let duplicate = ui.add_enabled(
        !entry.read_only,
        egui::Button::new(tr!("library-menu-duplicate")),
    );
    if duplicate.clicked() {
        picked = Some(Do::Out(super::Action::Run(
            "library.element.copy".into(),
            json!({ "element": entry.uuid, "library": entry.library }),
        )));
    }
    if ui.button(tr!("library-menu-where-used")).clicked() {
        picked = Some(Do::WhereUsed(entry.uuid));
    }
    if entry.part.is_some()
        && status.is_some_and(|s| s.placed > 0)
        && ui.button(tr!("library-menu-update")).clicked()
    {
        picked = Some(Do::update());
    }
    if ui.button(tr!("library-menu-check")).clicked() {
        picked = Some(Do::Check(entry.uuid));
    }
    if ui.button(tr!("library-menu-copy")).clicked() {
        picked = Some(Do::Out(super::Action::Call(
            "library.element.copy".into(),
            json!({ "element": entry.uuid }),
        )));
    }
    if let Some(url) = entry
        .part
        .as_ref()
        .and_then(|p| datasheet(&p.fields.datasheet))
        && ui.button(tr!("library-menu-datasheet")).clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
    if picked.is_some() {
        ui.close();
    }
    picked
}

/// A datasheet field that is a link.
fn datasheet(value: &str) -> Option<&str> {
    let value = value.trim();
    (value.starts_with("http://") || value.starts_with("https://")).then_some(value)
}
