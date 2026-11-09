//! The Footprint generator form: one modal, not a wizard. The family list, then the
//! sections of the generator's params (target, package, goals, tolerances, Pads,
//! outputs) with every value the generator calculates shown in its empty field, and a
//! live preview with dimension callouts and the generator's warnings. A typed value
//! overrides the calculated one; one that breaks IPC-7351 compliance is flagged where it
//! was typed. **Generate** works whenever the params are complete.
//!
//! Regenerate… in a generated Footprint's properties opens the same form on its stored
//! params, for `library.footprint.regenerate`; the preview then also lists what the
//! Footprint loses.

mod callouts;
mod draw;
mod form;

use std::any::Any;

use reda_engine::command::CommandError;
use reda_engine::library::{GeneratorWarning, RegenerateChange};
use reda_engine::model::{Footprint, LibraryElement, Uuid};
use serde_json::{Value, json};

use crate::command_dialog::{Outcome, ParamsDialog};
use crate::shell::Shell;

pub use callouts::{Axis, Callout, Side, callouts};
pub use form::{GeneratorForm, Row, Section, mm};

/// The Command a Regenerate… form runs.
pub const REGENERATE: &str = "library.footprint.regenerate";

/// A generator's form.
pub struct GenerateDialog {
    form: GeneratorForm,
    /// The Footprint being regenerated, for a Regenerate… form.
    regenerating: Option<Box<Footprint>>,
    library: Option<Uuid>,
    /// The Library the call gave, until the first frame picks the starting one.
    given_library: Option<Value>,
    error: Option<String>,
}

/// What the preview shows for the params so far.
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    /// The form is not complete yet.
    Incomplete,
    /// What the generator would write.
    Footprint(Box<Generated>),
    /// Why the params give no land pattern.
    Invalid(String),
}

/// What the generator would write with the form's params.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub footprint: Footprint,
    /// The params with every value resolved, as the Footprint would store them.
    pub resolved: Value,
    pub warnings: Vec<GeneratorWarning>,
    /// Regenerating: what the Footprint loses, and what it gains.
    pub lost: Vec<RegenerateChange>,
    pub added: Vec<RegenerateChange>,
    /// Regenerating: whether the Footprint would stay as it is.
    pub unchanged: Option<bool>,
}

/// How one row reads now.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// The value typed, else the one calculated or defaulted, as text.
    pub text: String,
    /// The value is not typed in: the generator calculates it or takes its default.
    pub calculated: bool,
    /// The typed value overrides IPC-7351's and the land pattern is no longer compliant.
    pub overrides_ipc: bool,
}

impl GenerateDialog {
    /// The dialog for generator `id`, filled in with what the call gave.
    pub fn new(shell: &Shell, id: &str, given: &Value) -> GenerateDialog {
        GenerateDialog {
            form: GeneratorForm::new(shell.engine().registry(), id, given),
            regenerating: None,
            library: None,
            given_library: Some(given["library"].clone()),
            error: None,
        }
    }

    /// The Regenerate… form for the open generated Footprint `footprint`, on its stored
    /// params; `None` when it is not open or no generator made it.
    pub fn regenerate(shell: &Shell, footprint: Uuid) -> Option<GenerateDialog> {
        let Some(LibraryElement::Footprint(old)) = shell.engine().element(footprint) else {
            return None;
        };
        let generator = old.generator.as_ref()?;
        let registry = shell.engine().registry();
        Some(GenerateDialog {
            form: GeneratorForm::stored(registry, &generator.command, &generator.params),
            library: generator.params["library"]
                .as_str()
                .and_then(|l| l.parse().ok()),
            regenerating: Some(Box::new(old.clone())),
            given_library: None,
            error: None,
        })
    }

    /// The form, for tests that fill it in.
    pub fn form(&self) -> &GeneratorForm {
        &self.form
    }

    pub fn form_mut(&mut self) -> &mut GeneratorForm {
        &mut self.form
    }

    pub fn library(&self) -> Option<Uuid> {
        self.library
    }

    /// The Footprint a Regenerate… form regenerates.
    pub fn regenerating(&self) -> Option<Uuid> {
        self.regenerating.as_ref().map(|f| f.uuid)
    }

    /// Switches the form to another generator family, keeping the target Library.
    pub fn set_family(&mut self, shell: &Shell, id: &str) {
        if self.regenerating.is_none() && id != self.form.command() {
            let library = self.form.given("/library");
            let given = library.map_or_else(|| json!({}), |l| json!({ "library": l }));
            self.form = GeneratorForm::new(shell.engine().registry(), id, &given);
        }
    }

    /// The generator's params from the form, checked, with a stand-in Library while
    /// none is picked: the preview does not need one.
    fn checked_params(&self, shell: &Shell) -> Option<Value> {
        let mut form = self.form.clone();
        if form.given("/library").is_none() {
            form.set_text("/library", &Uuid::NIL.to_string());
        }
        form.params(shell.engine().registry())
    }

    /// What the preview shows for the params so far.
    pub fn preview(&self, shell: &Shell) -> Preview {
        let Some(params) = self.checked_params(shell) else {
            return Preview::Incomplete;
        };
        if let Some(old) = &self.regenerating {
            let library = self.library.unwrap_or(Uuid::NIL);
            let mut next = u128::from(u32::MAX);
            let mut new_id = || {
                next += 1;
                Uuid::from_u128(next)
            };
            return match reda_engine::library::regenerate_footprint(
                old,
                library,
                Some(&params),
                &mut new_id,
            ) {
                Ok((footprint, out)) => Preview::Footprint(Box::new(Generated {
                    resolved: resolved(&footprint),
                    footprint,
                    warnings: out.warnings,
                    lost: out.lost,
                    added: out.added,
                    unchanged: Some(out.unchanged),
                })),
                Err(e) => Preview::Invalid(e.message),
            };
        }
        match reda_engine::library::preview_footprint(self.form.command(), &params) {
            Some(Ok(preview)) => Preview::Footprint(Box::new(Generated {
                resolved: resolved(&preview.footprint),
                footprint: preview.footprint,
                warnings: preview.warnings,
                lost: Vec::new(),
                added: Vec::new(),
                unchanged: None,
            })),
            Some(Err(e)) => Preview::Invalid(e.message),
            None => Preview::Incomplete,
        }
    }

    /// How the row at `pointer` reads with `preview`: what was typed, else the value the
    /// generator calculated for it.
    pub fn shown(&self, preview: &Preview, pointer: &str) -> Option<Shown> {
        let row = self.form.row(pointer)?;
        let generated = match preview {
            Preview::Footprint(g) => Some(g),
            _ => None,
        };
        let overrides_ipc =
            generated.is_some_and(|g| form::overridden(&g.warnings).iter().any(|p| p == pointer));
        if let Some(given) = self.form.given(pointer) {
            return Some(Shown {
                text: text(row, &given),
                calculated: false,
                overrides_ipc,
            });
        }
        let value = generated
            .and_then(|g| g.resolved.pointer(pointer).cloned())
            .or_else(|| row.default.clone())?;
        Some(Shown {
            text: text(row, &value),
            calculated: true,
            overrides_ipc: false,
        })
    }

    /// The params **Generate** (or **Regenerate**) runs its Command with, once the form
    /// is complete.
    pub fn run_params(&self, shell: &Shell) -> Option<Value> {
        let params = self.form.params(shell.engine().registry())?;
        Some(match &self.regenerating {
            Some(old) => json!({ "footprint": old.uuid, "params": params }),
            None => params,
        })
    }

    fn sync_library(&mut self) {
        if self.regenerating.is_some() {
            return;
        }
        let text = self.library.map(|l| l.to_string()).unwrap_or_default();
        self.form.set_text("/library", &text);
    }
}

/// The params a generated Footprint stores: every value resolved.
fn resolved(footprint: &Footprint) -> Value {
    footprint
        .generator
        .as_ref()
        .map_or(Value::Null, |g| g.params.clone())
}

/// A value as a row shows it: lengths in millimetres.
fn text(row: &Row, value: &Value) -> String {
    match (&row.field.input, value) {
        (crate::command_dialog::Input::Measure(_), Value::Number(n)) => {
            n.as_i64().map_or_else(|| n.to_string(), mm)
        }
        (_, Value::String(s)) => s.clone(),
        (_, other) => other.to_string(),
    }
}

impl ParamsDialog for GenerateDialog {
    fn command(&self) -> &str {
        match self.regenerating {
            Some(_) => REGENERATE,
            None => self.form.command(),
        }
    }

    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome {
        if let Some(given) = self.given_library.take() {
            self.library = super::initial_library(shell, &given);
        }
        self.sync_library();
        draw::show(self, ctx, shell)
    }

    fn failed(&mut self, error: &CommandError) {
        self.error = Some(error.message.clone());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
