//! The New Project dialog: name, location, the Board Template picker with its preview,
//! Fab Preset and the board's copper Layers. It builds the params of one `project.new`
//! Command. Add PCB Document ([`NewPcbForm`]) shows the same picker without the name and
//! location, for one `document.add`.

pub mod picker;
pub mod preview;

use std::sync::Arc;

use egui::{Id, Ui};
use reda_engine::library::LibraryIndex;
use reda_engine::model::FabPreset;
use reda_engine::render::Palette;
use serde_json::{Value, json};

use self::picker::Picker;
use self::preview::Look;
use crate::settings::{self, layout};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

/// The width of the creation dialogs.
const DIALOG_WIDTH: f32 = 1010.0;
/// The widths of the Name and Location fields.
const NAME_WIDTH: f32 = 220.0;
const LOCATION_WIDTH: f32 = 460.0;

/// The New Project dialog's fields.
#[derive(Clone)]
pub struct NewProjectForm {
    pub name: String,
    pub location: String,
    /// The Fab Presets to pick from: the shipped ones, then the user's.
    pub presets: Vec<FabPreset>,
    pub preset: String,
    pub layers: LayerCount,
    pub picker: Picker,
}

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormAction {
    None,
    Browse,
    Create,
    Cancel,
}

impl NewProjectForm {
    /// A dialog with a suggested name in the user's home folder, offering `presets` with
    /// `default` picked, and the Board Templates of `index` but its Project Libraries'.
    pub fn new(
        presets: Vec<FabPreset>,
        default: String,
        index: Arc<LibraryIndex>,
    ) -> NewProjectForm {
        NewProjectForm {
            name: tr!("new-project-default-name"),
            location: std::env::home_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            presets,
            preset: default,
            layers: LayerCount::default(),
            picker: Picker::new(index, false),
        }
    }

    /// The `project.new` params, or `None` while a field is missing or invalid, or the
    /// template's frame is.
    pub fn params(&mut self) -> Option<Value> {
        let (name, location) = (self.name.trim(), self.location.trim());
        if name.is_empty() || location.is_empty() {
            return None;
        }
        let mut params = json!({"path": location, "name": name, "fab_preset": self.preset});
        let template = self.picker.board_template().ok()?;
        self.layers.suggest(self.picker.suggested_layers());
        if let Some(layers) = self.layers.param()? {
            params["layers"] = json!(layers);
        }
        if let Some(name) = &self.layers.construction {
            params["construction"] = json!(name);
        }
        if let Some(template) = template {
            params["board_template"] = template;
        }
        Some(params)
    }

    /// Shows the dialog as a modal, its preview in `palette`'s Layer colours.
    pub fn show(&mut self, ctx: &egui::Context, palette: &Palette) -> FormAction {
        let mut action = FormAction::None;
        let modal =
            crate::modal::Dialog::new(Id::NULL.with("new-project"), tr!("new-project-title"))
                .width(DIALOG_WIDTH)
                .show(ctx, |ui| {
                    let t = ui.tokens();
                    let look = Look::new(ui, palette);
                    ui.horizontal(|ui| {
                        labelled_field(ui, &tr!("new-project-name"), &mut self.name, NAME_WIDTH);
                        ui.add_space(t.space.section);
                        labelled_field(
                            ui,
                            &tr!("new-project-location"),
                            &mut self.location,
                            LOCATION_WIDTH,
                        );
                        if ui.add(widgets::button(tr!("new-project-browse"))).clicked() {
                            action = FormAction::Browse;
                        }
                    });
                    ui.add_space(t.space.section);
                    self.layers.suggest(self.picker.suggested_layers());
                    let (presets, preset, layers) =
                        (&self.presets, &mut self.preset, &mut self.layers);
                    self.picker.show(ui, &look, |ui| {
                        settings::strong(ui, tr!("new-project-fab-preset"));
                        ui.horizontal_wrapped(|ui| {
                            for p in presets {
                                radio(ui, preset, p.name.clone(), &p.name);
                            }
                        });
                        layout::muted(ui, tr!("new-project-fab-preset-note"));
                        ui.add_space(t.space.stack);
                        if let Some(p) = presets.iter().find(|p| p.name == *preset) {
                            layers.show(ui, p);
                        }
                    });
                    crate::modal::buttons(ui, |ui| {
                        let ready = self.params().is_some();
                        if ui
                            .add_enabled(
                                ready,
                                widgets::button(tr!("new-project-create")).primary(),
                            )
                            .clicked()
                        {
                            action = FormAction::Create;
                        }
                        if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                            action = FormAction::Cancel;
                        }
                    });
                });
        if modal.should_close() && action == FormAction::None {
            action = FormAction::Cancel;
        }
        action
    }
}

fn labelled_field(ui: &mut Ui, label: &str, value: &mut String, width: f32) {
    let label = ui.add(widgets::form_name(label));
    let width = ui.tokens().size.of(width);
    ui.add(widgets::text_field(value).width(width))
        .labelled_by(label.id);
}

/// A radio button for `choice`: clicking it sets `value` to `choice`.
fn radio<T: PartialEq>(ui: &mut Ui, value: &mut T, choice: T, text: impl Into<String>) {
    if ui.add(widgets::radio(*value == choice, text)).clicked() {
        *value = choice;
    }
}

/// The copper Layer count entered when creating a board. A Board Template's suggestion
/// fills it until the user types their own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerCount {
    text: String,
    context: Option<(String, Option<u8>)>,
    pub construction: Option<String>,
    /// The picked template's suggestion, if there is one.
    suggested: Option<u8>,
    /// The user typed or picked a count: suggestions no longer change it.
    typed: bool,
}

impl Default for LayerCount {
    fn default() -> Self {
        Self {
            text: DEFAULT_LAYERS.to_string(),
            context: None,
            construction: None,
            suggested: None,
            typed: false,
        }
    }
}

/// The copper Layer count of a board made without a template.
const DEFAULT_LAYERS: u8 = 2;

impl LayerCount {
    pub fn value(&self) -> Option<u8> {
        self.text
            .trim()
            .parse()
            .ok()
            .filter(|n| (2..=32).contains(n))
    }

    /// Takes the picked template's suggestion (`None` for no template): it fills the
    /// field until the user types their own count.
    pub fn suggest(&mut self, suggested: Option<u8>) {
        self.suggested = suggested;
        if !self.typed {
            self.text = suggested.unwrap_or(DEFAULT_LAYERS).to_string();
        }
    }

    /// The `layers` param: `Some(None)` while the field holds the template's suggestion,
    /// which the Command then takes itself, and `None` while the count is invalid.
    pub fn param(&self) -> Option<Option<u8>> {
        let value = self.value()?;
        Some((self.suggested != Some(value)).then_some(value))
    }

    pub fn show(&mut self, ui: &mut Ui, preset: &FabPreset) {
        let before = self.text.clone();
        ui.horizontal(|ui| {
            let label = ui.add(widgets::form_name(tr!("stackup-copper-layers")));
            let width = ui.tokens().size.of(60.0);
            ui.add(widgets::text_field(&mut self.text).width(width))
                .labelled_by(label.id);
            layout::muted(ui, tr!("stackup-layer-range"));
        });
        if let Some(n) = self.suggested {
            layout::muted(
                ui,
                tr!("new-project-suggested-layers", count = i64::from(n)),
            );
        }
        let counts: std::collections::BTreeSet<_> = preset
            .constructions
            .iter()
            .map(|c| c.layer_count())
            .collect();
        ui.horizontal_wrapped(|ui| {
            for count in counts {
                if ui.add(widgets::button(count.to_string())).clicked() {
                    self.text = count.to_string();
                }
            }
        });
        if self.text != before {
            self.typed = true;
        }
        let count = self.value();
        let context = (preset.name.clone(), count);
        if self.context.as_ref() != Some(&context) {
            self.construction = count
                .and_then(|n| preset.default_construction(usize::from(n)))
                .map(|c| c.name.clone());
            self.context = Some(context);
        }
        let mut builds: Vec<_> = preset
            .constructions
            .iter()
            .filter(|c| Some(c.layer_count()) == count.map(usize::from))
            .map(|c| (Some(c.name.clone()), c.name.clone()))
            .collect();
        if !builds.is_empty() {
            if !preset
                .constructions
                .iter()
                .any(|c| c.default && Some(c.layer_count()) == count.map(usize::from))
            {
                builds.insert(0, (None, tr!("construction-generic")));
            }
            ui.horizontal(|ui| {
                let label = tr!("construction-title");
                ui.add(widgets::form_name(&label));
                settings::dropdown(
                    ui,
                    "board-construction",
                    &label,
                    &mut self.construction,
                    &builds,
                    200.,
                );
            });
        } else if count.is_some() {
            layout::muted(ui, tr!("construction-generic"));
        }
    }
}

/// The Project panel's Add PCB Document dialog: the same picker, listing the Project's
/// Libraries too, with the Fab Preset (the Project's copies first) and the build.
#[derive(Clone, Debug)]
pub struct NewPcbForm {
    presets: Vec<(Option<reda_engine::model::Uuid>, FabPreset)>,
    selected: usize,
    layers: LayerCount,
    pub picker: Picker,
}

impl NewPcbForm {
    pub fn new(
        project: &reda_engine::model::Project,
        dirs: Option<&reda_engine::command::UserDirs>,
        index: Arc<LibraryIndex>,
    ) -> Self {
        let mut presets: Vec<_> = project
            .fab_presets()
            .map(|copy| (Some(copy.uuid), copy.preset()))
            .collect();
        for source in reda_engine::fab_presets::available(dirs).0 {
            presets.push((None, source.preset));
        }
        let selected = presets
            .iter()
            .position(|(id, p)| id.is_none() && p.name == reda_engine::output::GENERIC_NAME)
            .unwrap_or_default();
        Self {
            presets,
            selected,
            layers: LayerCount::default(),
            picker: Picker::new(index, true),
        }
    }

    /// The dialog's width.
    pub const WIDTH: f32 = DIALOG_WIDTH;

    /// Draws the picker, with the Fab Preset and the build under its parameters.
    pub fn show(&mut self, ui: &mut Ui, palette: &Palette) {
        let look = Look::new(ui, palette);
        let choices: Vec<_> = self
            .presets
            .iter()
            .enumerate()
            .map(|(index, (copy, preset))| {
                let label = if copy.is_some() {
                    tr!("construction-project-copy", name = preset.name.as_str())
                } else {
                    preset.name.clone()
                };
                (index, label)
            })
            .collect();
        self.layers.suggest(self.picker.suggested_layers());
        let (presets, selected, layers) = (&self.presets, &mut self.selected, &mut self.layers);
        self.picker.show(ui, &look, |ui| {
            ui.horizontal(|ui| {
                let label = tr!("new-project-fab-preset");
                ui.add(widgets::form_name(&label));
                let old = *selected;
                settings::dropdown(ui, "pcb-fab-preset", &label, selected, &choices, 200.);
                if *selected != old {
                    layers.context = None;
                }
            });
            if let Some((_, preset)) = presets.get(*selected) {
                layers.show(ui, preset);
            }
        });
    }

    /// The `document.add` params, or `None` while a field or the template's frame is
    /// invalid.
    pub fn params(&mut self) -> Option<Value> {
        let template = self.picker.board_template().ok()?;
        self.layers.suggest(self.picker.suggested_layers());
        let (copy, preset) = self.presets.get(self.selected)?;
        let key = copy
            .map(|id| id.to_string())
            .unwrap_or_else(|| preset.name.clone());
        let mut params = json!({"kind": "pcb", "fab_preset": key});
        if let Some(layers) = self.layers.param()? {
            params["layers"] = json!(layers);
        }
        if let Some(name) = &self.layers.construction {
            params["construction"] = json!(name);
        }
        if let Some(template) = template {
            params["board_template"] = template;
        }
        Some(params)
    }
}
