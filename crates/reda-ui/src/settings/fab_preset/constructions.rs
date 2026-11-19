//! Construction list shared by Project Settings and user Fab Preset Preferences.
use crate::{
    settings::{dropdown, field, layout, strong},
    tr, widgets,
};
use egui::Ui;
use reda_engine::model::FabPreset;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct List {
    selected: String,
    name: String,
    layers: String,
}

impl Default for List {
    fn default() -> Self {
        Self {
            selected: String::new(),
            name: String::new(),
            layers: "2".into(),
        }
    }
}

impl List {
    pub(super) fn show(&mut self, ui: &mut Ui, preset: &mut FabPreset) {
        strong(ui, tr!("construction-list"));
        if preset.construction(&self.selected).is_none() {
            self.selected = preset
                .constructions
                .first()
                .map(|b| b.name.clone())
                .unwrap_or_default();
        }
        let choices: Vec<_> = preset
            .constructions
            .iter()
            .map(|b| {
                let mut label = tr!(
                    "construction-count",
                    name = b.name.as_str(),
                    layers = b.layer_count()
                );
                if b.default {
                    label.push(' ');
                    label.push_str(&tr!("form-default"));
                }
                (b.name.clone(), label)
            })
            .collect();
        if !choices.is_empty() {
            dropdown(
                ui,
                "preset-build",
                &tr!("construction-title"),
                &mut self.selected,
                &choices,
                250.,
            );
        }
        let current = preset
            .constructions
            .iter()
            .position(|b| b.name == self.selected);
        let (set_default, remove) = if let Some(build) = current.map(|i| &preset.constructions[i]) {
            if let Some(notes) = &build.notes {
                layout::text(ui, notes);
            }
            ui.horizontal_wrapped(|ui| {
                let set_default = ui
                    .add_enabled(!build.default, widgets::button(tr!("construction-default")))
                    .clicked();
                let remove = ui
                    .add(widgets::button(tr!("construction-remove")))
                    .clicked();
                (set_default, remove)
            })
            .inner
        } else {
            (false, false)
        };
        if let Some(i) = current {
            let count = preset.constructions[i].layer_count();
            if set_default {
                for (n, build) in preset
                    .constructions
                    .iter_mut()
                    .enumerate()
                    .filter(|(_, b)| b.layer_count() == count)
                {
                    build.default = n == i;
                }
            } else if remove {
                let was_default = preset.constructions.remove(i).default;
                if was_default
                    && let Some(next) = preset
                        .constructions
                        .iter_mut()
                        .find(|b| b.layer_count() == count)
                {
                    next.default = true;
                }
                return;
            }
        }
        widgets::form("new-build").show(ui, |ui| {
            let label = tr!("construction-name");
            ui.add(widgets::form_name(&label));
            field(ui, &label, &mut self.name, 180., None);
            ui.end_row();
            let label = tr!("stackup-copper-layers");
            ui.add(widgets::form_name(&label));
            field(ui, &label, &mut self.layers, 60., None);
            ui.end_row();
        });
        let name = self.name.trim();
        let free = !name.is_empty() && preset.construction(name).is_none();
        let count = self
            .layers
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|n| (2..=32).contains(n));
        let added = ui
            .horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        free && count.is_some(),
                        widgets::button(tr!("construction-add")),
                    )
                    .clicked()
                {
                    count.map(|n| {
                        reda_engine::pcb::construction::evenly_spaced(n, Default::default())
                    })
                } else if ui
                    .add_enabled(
                        free && current.is_some(),
                        widgets::button(tr!("construction-duplicate")),
                    )
                    .clicked()
                {
                    current.and_then(|i| preset.constructions.get(i)).cloned()
                } else {
                    None
                }
            })
            .inner;
        if let Some(mut build) = added {
            build.name = name.to_owned();
            build.default = preset.default_construction(build.layer_count()).is_none();
            self.selected = build.name.clone();
            preset.constructions.push(build);
            self.name.clear();
        }
    }
}
