//! The snap palette: the grid toggle (Gridless), the object snap mode and the Hotspot
//! checklist of one Document. It pops up from its Active Bar button, and `Mod`+E
//! (`view.snap.palette`) opens the same pop-up during any operation. Every change is a
//! `view.snap.set`.

use std::collections::BTreeMap;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::Uuid;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{hotspot_label, hotspot_name, offered};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::widgets::{self, Glyph, Tooltip as _};
use crate::working_state::{SnapMode, WorkingState};

/// Adds `view.snap.palette`.
pub fn register(registry: &mut Registry) {
    registry.add(OpenPalette);
}

/// How many times `view.snap.palette` ran per Document: each run toggles that canvas's
/// pop-up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaletteRequests(pub BTreeMap<Uuid, u64>);

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaletteParams {
    /// The Document, or open Library element, whose palette to open or close.
    document: Uuid,
}

struct OpenPalette;

impl Command for OpenPalette {
    const META: Meta = Meta {
        id: "view.snap.palette",
        title: "Snap Palette",
        description: "Opens, or closes, the active Document's snap palette: the grid \\
                      toggle, the object snap mode and the Hotspot checklist. It works \\
                      during any operation.",
        category: "view",
        icon: "grid",
        kind: Kind::View,
    };
    type Params = PaletteParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: PaletteParams) -> Result<(), CommandError> {
        *cx.view::<PaletteRequests>()?
            .0
            .entry(p.document)
            .or_default() += 1;
        Ok(())
    }
}

/// The palette's controls for `document`, as the `view.snap.set` calls they make.
pub fn controls(ui: &mut egui::Ui, shell: &Shell, document: Uuid) -> Vec<(&'static str, Value)> {
    let Some(ty) = crate::bars::document_type(shell, document) else {
        return Vec::new();
    };
    let options = shell
        .engine()
        .view()
        .get::<WorkingState>()
        .map(|w| w.snap(document, ty))
        .unwrap_or_else(|| crate::working_state::SnapOptions::default_for(ty));
    let mut commands = Vec::new();
    let mut grid = !options.gridless;
    let grid_box = widgets::checkbox(&mut grid, crate::tr!("snap-palette-grid"));
    if ui.add(grid_box).changed() {
        commands.push((
            "view.snap.set",
            json!({"document": document, "gridless": !grid}),
        ));
    }
    ui.separator();
    ui.label(crate::tr!("snap-palette-mode"));
    for &mode in SnapMode::offered(ty) {
        let radio = widgets::radio(options.mode == mode, mode_label(mode));
        if ui.add(radio).clicked() && options.mode != mode {
            commands.push(("view.snap.set", json!({"document": document, "mode": mode})));
        }
    }
    ui.separator();
    ui.label(crate::tr!("snap-palette-hotspots"));
    for &kind in offered(ty) {
        let mut on = !options.hotspots_off.contains(&kind);
        if ui
            .add(widgets::checkbox(&mut on, hotspot_label(kind)))
            .changed()
        {
            commands.push((
                "view.snap.set",
                json!({"document": document, "hotspots": {hotspot_name(kind): on}}),
            ));
        }
    }
    commands
}

/// The mode's words in the palette.
pub fn mode_label(mode: SnapMode) -> String {
    match mode {
        SnapMode::ActiveLayer => crate::tr!("snap-mode-active-layer"),
        SnapMode::AllLayers => crate::tr!("snap-mode-all-layers"),
        SnapMode::On => crate::tr!("snap-mode-on"),
        SnapMode::Off => crate::tr!("snap-mode-off"),
    }
}

/// The Active Bar's snap palette button for `document`, with its pop-up. `Mod`+E opens
/// the same pop-up.
pub fn palette_button(ui: &mut egui::Ui, shell: &Shell, document: Uuid) -> Option<(String, Value)> {
    let title = crate::tr!("snap-palette-title");
    let button = widgets::glyph_button(Glyph::Snap).side(crate::chrome::BAR_BUTTON);
    let response = ui.push_id(("snap-palette-button", document), |ui| ui.add(button));
    let response = response.inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &title));
    let response = response.tooltip(&title);
    let requested = shell
        .engine()
        .view()
        .get::<PaletteRequests>()
        .and_then(|r| r.0.get(&document).copied())
        .unwrap_or_default();
    let seen_id = egui::Id::NULL.with(("snap-palette-seen", document));
    let seen = ui.data(|d| d.get_temp::<u64>(seen_id)).unwrap_or(requested);
    ui.data_mut(|d| d.insert_temp(seen_id, requested));
    // A click on the button toggles the pop-up by itself; the Command toggles it too.
    let mut command = None;
    widgets::menu(&response)
        .toggle(seen != requested)
        .stay_open()
        .named(title.clone())
        .show(|ui| {
            let t = ui.tokens();
            ui.label(t.ty.body_strong.rich(&title).color(t.color.text.primary));
            command = controls(ui, shell, document)
                .into_iter()
                .next()
                .map(|(id, p)| (id.into(), p));
        });
    command
}
