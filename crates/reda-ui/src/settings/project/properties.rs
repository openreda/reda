//! The Properties page: the title block fields. Applied with `project.properties.set`.
//! The board thickness and copper are the Stackup's, set in Board Setup.

use egui::Ui;
use reda_engine::model::Project;
use serde_json::{Map, Value};

use crate::settings::{Draft, Page, PageCx, Problem, Step, field};
use crate::tr;

/// The fields of the Properties page, as typed.
#[derive(Clone, Debug, Default)]
pub struct PropertiesPage {
    pub title: Draft<String>,
    pub revision: Draft<String>,
    pub date: Draft<String>,
    pub author: Draft<String>,
    pub company: Draft<String>,
}

impl Page for PropertiesPage {
    fn title(&self) -> String {
        tr!("settings-page-properties")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        let p = project.props().properties.clone();
        self.title.load(p.title, reset);
        self.revision.load(p.revision, reset);
        self.date.load(p.date, reset);
        self.author.load(p.author, reset);
        self.company.load(p.company, reset);
    }

    fn steps(&self) -> Vec<Step> {
        let mut params = Map::new();
        for (name, draft) in [
            ("title", &self.title),
            ("revision", &self.revision),
            ("date", &self.date),
            ("author", &self.author),
            ("company", &self.company),
        ] {
            if draft.changed() {
                params.insert(name.to_owned(), Value::from(draft.value().clone()));
            }
        }
        if params.is_empty() {
            return Vec::new();
        }
        vec![Step::new("project.properties.set", Value::Object(params))]
    }

    fn has_defaults(&self) -> bool {
        true
    }

    /// Every field empty, as a new Project has them.
    fn reset_to_defaults(&mut self, _shell: &mut crate::shell::Shell) {
        for draft in [
            &mut self.title,
            &mut self.revision,
            &mut self.date,
            &mut self.author,
            &mut self.company,
        ] {
            draft.set(String::new());
        }
    }

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        egui::Grid::new("settings-properties")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                for (label, draft, field_name) in [
                    (tr!("settings-title"), &mut self.title, "title"),
                    (tr!("settings-revision"), &mut self.revision, "revision"),
                    (tr!("settings-date"), &mut self.date, "date"),
                    (tr!("settings-author"), &mut self.author, "author"),
                    (tr!("settings-company"), &mut self.company, "company"),
                ] {
                    ui.label(&label);
                    field(ui, &label, draft.value_mut(), 280.0, cx.problem(field_name));
                    ui.end_row();
                }
            });
    }
}
