//! The Properties page: the title block fields, and the board thickness and copper weight
//! for fab notes. Applied with `project.properties.set`.

use egui::Ui;
use reda_engine::model::Project;
use reda_engine::model::geom::Nm;
use serde_json::{Map, Value};

use crate::settings::{Draft, Page, PageCx, Problem, Step, field};
use crate::tr;

/// The fields of the Properties page, as typed. The board thickness is in mm unless it
/// names a unit (`1.6`, `1.6mm`, `63mil`); empty means not set.
#[derive(Clone, Debug, Default)]
pub struct PropertiesPage {
    pub title: Draft<String>,
    pub revision: Draft<String>,
    pub date: Draft<String>,
    pub author: Draft<String>,
    pub company: Draft<String>,
    pub board_thickness: Draft<String>,
    pub copper_weight: Draft<String>,
}

impl PropertiesPage {
    /// The text fields with their Command param names.
    fn text_fields(&self) -> [(&'static str, &Draft<String>); 6] {
        [
            ("title", &self.title),
            ("revision", &self.revision),
            ("date", &self.date),
            ("author", &self.author),
            ("company", &self.company),
            ("copper_weight", &self.copper_weight),
        ]
    }
}

/// `nm` in mm, with no trailing zeros (`1600000` → `1.6`).
fn mm(nm: Nm) -> String {
    let sign = if nm.0 < 0 { "-" } else { "" };
    let abs = nm.0.unsigned_abs();
    let (whole, frac) = (abs / 1_000_000, abs % 1_000_000);
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let frac = format!("{frac:06}");
    format!("{sign}{whole}.{}", frac.trim_end_matches('0'))
}

/// The board thickness typed: `Ok(None)` when empty, else a positive length.
fn thickness(text: &str) -> Result<Option<Nm>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let with_unit = if text.ends_with(|c: char| c.is_ascii_digit() || c == '.') {
        format!("{text}mm")
    } else {
        text.to_owned()
    };
    match Nm::parse(&with_unit) {
        Ok(nm) if nm.0 > 0 => Ok(Some(nm)),
        Ok(_) => Err(tr!("settings-thickness-positive")),
        Err(_) => Err(tr!("settings-thickness-invalid")),
    }
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
        self.copper_weight.load(p.copper_weight, reset);
        let typed = p.board_thickness.map(mm).unwrap_or_default();
        // "1.60" and "1.6mm" both mean what the Project has: not a change.
        let same = thickness(self.board_thickness.value()).ok() == Some(p.board_thickness);
        self.board_thickness.load(typed, reset || same);
    }

    fn steps(&self) -> Vec<Step> {
        let mut params = Map::new();
        for (name, draft) in self.text_fields() {
            if draft.changed() {
                params.insert(name.to_owned(), Value::from(draft.value().clone()));
            }
        }
        let base = thickness(self.board_thickness.base()).ok().flatten();
        if let Ok(value) = thickness(self.board_thickness.value())
            && value != base
        {
            params.insert(
                "board_thickness".to_owned(),
                Value::from(value.map_or(0, |nm| nm.0)),
            );
        }
        if params.is_empty() {
            return Vec::new();
        }
        vec![Step::new("project.properties.set", Value::Object(params))]
    }

    fn problems(&self) -> Vec<Problem> {
        match thickness(self.board_thickness.value()) {
            Ok(_) => Vec::new(),
            Err(message) => vec![Problem::new("board_thickness", message)],
        }
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
                    (
                        tr!("settings-board-thickness"),
                        &mut self.board_thickness,
                        "board_thickness",
                    ),
                    (
                        tr!("settings-copper-weight"),
                        &mut self.copper_weight,
                        "copper_weight",
                    ),
                ] {
                    ui.label(&label);
                    field(ui, &label, draft.value_mut(), 280.0, cx.problem(field_name));
                    ui.end_row();
                }
            });
        ui.add_space(6.0);
        ui.weak(tr!("settings-fab-notes"));
    }
}
