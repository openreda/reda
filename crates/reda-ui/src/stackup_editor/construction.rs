//! Save the current physical build in a Project copy or an existing user preset.
use crate::{
    modal,
    settings::{dropdown, field, layout},
    shell::Shell,
    tr, widgets,
};
use egui::Ui;
use reda_engine::model::{Construction, Project, Uuid};
use serde_json::json;

#[derive(Clone, Debug, PartialEq)]
enum Target {
    Project(Uuid),
    User(String),
}

#[derive(Clone)]
pub(super) struct SaveForm {
    targets: Vec<(Target, String)>,
    target: Option<Target>,
    name: String,
    error: Option<String>,
}

impl SaveForm {
    pub fn new(project: &Project, dirs: Option<&reda_engine::command::UserDirs>) -> Self {
        let mut targets: Vec<_> = project
            .fab_presets()
            .map(|p| {
                (
                    Target::Project(p.uuid),
                    tr!("construction-project-copy", name = p.name.as_str()),
                )
            })
            .collect();
        if let Some(dirs) = dirs {
            targets.extend(
                reda_engine::fab_presets::user_presets(dirs)
                    .0
                    .into_iter()
                    .map(|user| {
                        let name = user.preset.name;
                        let label = tr!("construction-user-preset", name = name.as_str());
                        (Target::User(name), label)
                    }),
            );
        }
        Self {
            target: targets.first().map(|(key, _)| key.clone()),
            targets,
            name: String::new(),
            error: None,
        }
    }

    pub fn show(&mut self, ui: &Ui, document: Uuid, shell: &mut Shell) -> bool {
        let mut save = false;
        let mut cancel = false;
        let dialog = modal::Dialog::new(
            egui::Id::NULL.with(("save-construction", document)),
            tr!("construction-save-title"),
        )
        .show(ui.ctx(), |ui| {
            widgets::form("save-build").show(ui, |ui| {
                let label = tr!("new-project-fab-preset");
                ui.add(widgets::form_name(&label));
                if let Some(target) = &mut self.target {
                    dropdown(
                        ui,
                        "construction-destination",
                        &label,
                        target,
                        &self.targets,
                        220.,
                    );
                } else {
                    layout::muted(ui, tr!("construction-no-presets"));
                }
                ui.end_row();
                let label = tr!("construction-name");
                ui.add(widgets::form_name(&label));
                field(ui, &label, &mut self.name, 210., None);
                ui.end_row();
            });
            if let Some(error) = &self.error {
                layout::error(ui, error);
            }
            modal::buttons(ui, |ui| {
                save = ui
                    .add_enabled(
                        self.target.is_some() && !self.name.trim().is_empty(),
                        widgets::button(tr!("construction-save")).primary(),
                    )
                    .clicked();
                cancel = ui.add(widgets::button(tr!("dialog-cancel"))).clicked();
            });
        });
        if cancel || dialog.should_close() {
            return true;
        }
        if !save {
            return false;
        }
        let name = self.name.trim();
        let result = match self.target.as_ref().expect("selected destination") {
            Target::Project(preset) => shell.run(
                "pcb.stackup.save_as_construction",
                json!({"document":document,"preset":preset,"name":name}),
            ),
            Target::User(preset) => {
                let dirs = shell
                    .engine()
                    .user_dirs()
                    .expect("user preset has user dirs");
                let source = match reda_engine::fab_presets::user_preset(dirs, preset) {
                    Ok((source, _)) => source,
                    Err(error) => {
                        self.error = Some(error.message);
                        return false;
                    }
                };
                let pcb = shell
                    .engine()
                    .project()
                    .and_then(|p| p.pcb_document(document))
                    .expect("open board tab");
                let build =
                    Construction::capture(name.to_owned(), pcb.stackup(), &source.constructions);
                shell.run(
                    "fab_preset.set_user",
                    json!({"name":preset,"values":{"constructions":{(name):build}}}),
                )
            }
        };
        match result {
            Ok(_) => true,
            Err(error) => {
                self.error = Some(error.message);
                false
            }
        }
    }
}
