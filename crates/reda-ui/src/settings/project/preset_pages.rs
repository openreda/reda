//! The pages of one Project copy of a Fab Preset (Rules, Stackup, Gerber, Drill, BOM,
//! CPL, Packaging), under the copy's name in Project Settings. A copy's pages share one
//! draft; each page commits its own keys on Apply with `fab_preset.set`.

use std::cell::RefCell;
use std::rc::Rc;

use egui::Ui;
use reda_engine::model::{Project, Uuid};
use serde_json::json;

use crate::settings::fab_preset::{self, PresetEditor, PresetPage};
use crate::settings::{Page, PageCx, Problem, Step};
use crate::shell::Shell;
use crate::tr;

/// One page of one Project copy.
pub struct CopyPage {
    preset: Uuid,
    name: String,
    page: PresetPage,
    editor: Rc<RefCell<PresetEditor>>,
    /// Whether the copy is still in the Project.
    present: bool,
}

impl CopyPage {
    /// The seven pages of the copy `preset` in `project`, sharing one draft.
    pub fn all(project: &Project, preset: Uuid) -> Vec<CopyPage> {
        let Some(copy) = project.fab_preset(preset) else {
            return Vec::new();
        };
        let editor = Rc::new(RefCell::new(PresetEditor::new(copy.preset())));
        PresetPage::ALL
            .into_iter()
            .map(|page| CopyPage {
                preset,
                name: copy.name.clone(),
                page,
                editor: editor.clone(),
                present: true,
            })
            .collect()
    }

    pub fn preset(&self) -> Uuid {
        self.preset
    }

    pub fn page(&self) -> PresetPage {
        self.page
    }

    /// The copy's draft, shared by its pages.
    pub fn editor(&self) -> &Rc<RefCell<PresetEditor>> {
        &self.editor
    }
}

impl Page for CopyPage {
    fn title(&self) -> String {
        self.page.title()
    }

    fn group(&self) -> Option<String> {
        Some(tr!("settings-preset-group", name = self.name.as_str()))
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.present = project.fab_preset(self.preset).is_some();
        // The copy's first page loads the shared draft for all of them.
        if self.page != PresetPage::ALL[0] {
            return;
        }
        if let Some(copy) = project.fab_preset(self.preset) {
            self.editor.borrow_mut().load(copy.preset(), reset);
        }
    }

    fn steps(&self) -> Vec<Step> {
        if !self.present {
            return Vec::new();
        }
        self.editor
            .borrow()
            .changes(self.page)
            .map(|values| {
                Step::new(
                    "fab_preset.set",
                    json!({ "preset": self.preset, "values": values }),
                )
            })
            .into_iter()
            .collect()
    }

    fn problems(&self) -> Vec<Problem> {
        self.editor.borrow().problems(self.page)
    }

    fn has_defaults(&self) -> bool {
        true
    }

    fn reset_label(&self) -> String {
        tr!("preset-reset-page")
    }

    /// Reset page to source: the page's keys take the values of the preset the copy came
    /// from (a shipped or user preset of its name).
    fn reset_to_defaults(&mut self, shell: &mut Shell) {
        let source = reda_engine::fab_presets::find(&self.name, shell.engine().user_dirs());
        if let Some(source) = source {
            self.editor
                .borrow_mut()
                .reset_page(self.page, &source.preset);
        }
    }

    fn show(&mut self, ui: &mut Ui, _cx: &mut PageCx<'_>) {
        if !self.present {
            ui.weak(tr!("settings-preset-gone"));
            return;
        }
        ui.weak(tr!("settings-preset-copy-of", name = self.name.as_str()));
        ui.add_space(4.0);
        fab_preset::draw::page(ui, &mut self.editor.borrow_mut(), self.page);
    }
}
