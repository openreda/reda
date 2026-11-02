//! The Libraries page: the Project Library paths, relative to the Project folder. Applied
//! with `project.library.remove` and `project.library.add`.

use std::path::Path;

use egui::Ui;
use reda_engine::library::normalise_library_path;
use reda_engine::model::Project;
use serde_json::json;

use crate::settings::{Draft, Page, PageCx, Problem, Step, field};
use crate::shell::Shell;
use crate::tr;

/// The Library paths as typed, one row each.
#[derive(Clone, Debug, Default)]
pub struct LibrariesPage {
    rows: Draft<Vec<String>>,
    /// The New Library name being typed.
    pub new_name: String,
    /// What the last New Library did: the folder it made, or why it failed.
    created: Option<Result<String, String>>,
}

impl LibrariesPage {
    pub fn rows(&self) -> &[String] {
        self.rows.value()
    }

    /// Adds a row.
    pub fn add(&mut self, path: &str) {
        self.rows.value_mut().push(path.to_owned());
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.rows.value().len() {
            self.rows.value_mut().remove(index);
        }
    }

    /// Creates an empty Library named `name` in a folder of that name in the Project
    /// folder (`library.create`, at once, since it writes files) and adds its path as a
    /// row, which Apply registers with the Project.
    pub fn create(&mut self, shell: &mut Shell, name: &str) -> Result<String, String> {
        let params = json!({"path": name.trim(), "name": name.trim()});
        let out = shell
            .run("library.create", params)
            .map_err(|e| e.message.clone())?;
        let path = out["project_path"]
            .as_str()
            .unwrap_or(name.trim())
            .to_owned();
        self.add(&path);
        Ok(path)
    }

    /// Each row's stored form, or why it is invalid.
    fn checked(&self) -> Vec<Result<String, String>> {
        let mut seen: Vec<String> = Vec::new();
        self.rows
            .value()
            .iter()
            .map(|row| {
                if row.trim().is_empty() {
                    return Err(tr!("settings-library-blank"));
                }
                let path = normalise_library_path(row.trim())
                    .map_err(|why| tr!("settings-library-invalid", reason = why))?;
                if seen.contains(&path) {
                    return Err(tr!("settings-library-duplicate", path = path.as_str()));
                }
                seen.push(path.clone());
                Ok(path)
            })
            .collect()
    }
}

impl Page for LibrariesPage {
    fn title(&self) -> String {
        tr!("settings-page-libraries")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.rows.load(project.props().libraries.clone(), reset);
    }

    fn steps(&self) -> Vec<Step> {
        let wanted: Vec<String> = self.checked().into_iter().flatten().collect();
        let base = self.rows.base();
        let removes = base
            .iter()
            .filter(|p| !wanted.contains(p))
            .map(|p| Step::new("project.library.remove", json!({ "path": p })));
        let adds = wanted
            .iter()
            .filter(|p| !base.contains(p))
            .map(|p| Step::new("project.library.add", json!({ "path": p })));
        removes.chain(adds).collect()
    }

    fn problems(&self) -> Vec<Problem> {
        self.checked()
            .into_iter()
            .enumerate()
            .filter_map(|(i, r)| r.err().map(|why| Problem::new(format!("path/{i}"), why)))
            .collect()
    }

    fn show(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        ui.weak(tr!("settings-libraries-note"));
        ui.add_space(4.0);
        let mut remove = None;
        let count = self.rows.value().len();
        if count == 0 {
            ui.weak(tr!("projects-no-libraries"));
        }
        for i in 0..count {
            let problem = cx.problem(&format!("path/{i}")).cloned();
            ui.horizontal_top(|ui| {
                let label = tr!("settings-library-path", number = i + 1);
                field(
                    ui,
                    &label,
                    &mut self.rows.value_mut()[i],
                    360.0,
                    problem.as_ref(),
                );
                let row = tr!("settings-library-remove", number = i + 1);
                let button = ui.button(tr!("projects-remove"));
                button.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, row.clone())
                });
                if button.clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.remove(i);
        }
        ui.horizontal(|ui| {
            if ui.button(tr!("settings-library-add-path")).clicked() {
                self.add("");
            }
            if ui.button(tr!("projects-add-folder")).clicked() {
                cx.pick_folder();
            }
        });
        ui.horizontal(|ui| {
            let label = tr!("settings-library-new-name");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_name)
                    .hint_text(label.clone())
                    .desired_width(200.0),
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone())
            });
            let create = ui.add_enabled(
                !self.new_name.trim().is_empty(),
                egui::Button::new(tr!("settings-library-new")),
            );
            if create.clicked() {
                let name = std::mem::take(&mut self.new_name);
                self.created = Some(self.create(cx.shell, &name));
            }
        });
        match &self.created {
            Some(Ok(path)) => {
                ui.weak(tr!("settings-library-created", path = path.as_str()));
            }
            Some(Err(why)) => {
                ui.colored_label(ui.visuals().error_fg_color, why);
            }
            None => {}
        }
    }

    fn picked_folder(&mut self, path: &Path) {
        self.add(&path.to_string_lossy().replace('\\', "/"));
    }
}
