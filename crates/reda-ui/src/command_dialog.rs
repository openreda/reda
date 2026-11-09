//! Calling a Command from the GUI when its params may be incomplete.
//!
//! A call with every required param runs at once and never opens a dialog. A call that
//! lacks one opens a dialog for the rest: the [`ParamForm`] generated from the Command's
//! params schema, or a hand-written dialog the Command registered with
//! [`Dialogs::register`] (as Sync's Change List will). The window shows the open dialog
//! each frame and runs the params it gives back.

mod draw;
pub(crate) mod form;

use std::any::Any;
use std::collections::BTreeMap;

use reda_engine::command::{CommandError, Registry};
use serde_json::Value;

use crate::shell::Shell;

pub use form::{Field, Input, ParamForm};

/// A dialog that collects a Command's params.
pub trait ParamsDialog: Any {
    /// The Command (or Preset) id the params are for.
    fn command(&self) -> &str;

    /// Draws the dialog for one frame.
    fn show(&mut self, ctx: &egui::Context, shell: &mut Shell) -> Outcome;

    /// The params from [`Outcome::Run`] failed; the dialog stays open and may say why.
    fn failed(&mut self, _error: &CommandError) {}

    /// The params from [`Outcome::Run`] succeeded with `result`: a Command the window
    /// runs next, such as registering a Library the dialog just created.
    fn then(&mut self, _result: &Value) -> Option<(String, Value)> {
        None
    }

    /// The dialog itself, for tests that look inside.
    fn as_any(&self) -> &dyn Any;
}

/// What the user did with a dialog this frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// Still filling it in.
    Open,
    /// Closed without running anything.
    Cancel,
    /// Run the Command with these params.
    Run(Value),
}

/// What a GUI call did.
pub enum Call {
    /// The Command ran (errors are also reported in the status bar).
    Ran(Result<Value, CommandError>),
    /// Params are missing: show this dialog until it gives them.
    Dialog(Box<dyn ParamsDialog>),
}

type Factory = Box<dyn Fn(&Shell, Value) -> Box<dyn ParamsDialog>>;

/// The hand-written dialogs that replace generated forms, by Command id.
#[derive(Default)]
pub struct Dialogs {
    hand_written: BTreeMap<String, Factory>,
    /// Commands whose dialog opens on every GUI call, as an options dialog does, even
    /// when no required param is missing.
    always: std::collections::BTreeSet<String>,
}

impl Dialogs {
    /// No hand-written dialogs: every Command gets its generated form.
    pub fn new() -> Dialogs {
        Dialogs::default()
    }

    /// Replaces the generated form of `command` with the dialog `make` builds from the
    /// params the call did give.
    pub fn register(
        &mut self,
        command: &str,
        make: impl Fn(&Shell, Value) -> Box<dyn ParamsDialog> + 'static,
    ) {
        self.hand_written.insert(command.to_owned(), Box::new(make));
    }

    /// Like [`Dialogs::register`], but the dialog opens on every GUI call of `command`,
    /// even when the call gives every required param: a Command whose params are all
    /// options with defaults, such as Silkscreen Preparation's, still asks first.
    pub fn register_always(
        &mut self,
        command: &str,
        make: impl Fn(&Shell, Value) -> Box<dyn ParamsDialog> + 'static,
    ) {
        self.register(command, make);
        self.always.insert(command.to_owned());
    }

    /// Whether `command`'s dialog opens on every GUI call ([`Dialogs::register_always`]).
    pub fn asks_always(&self, command: &str) -> bool {
        self.always.contains(command)
    }

    /// Calls `id` from the GUI: runs it when no required param is missing (or when it
    /// cannot run now, to report why), else returns the dialog that asks for the rest.
    /// A Command registered with [`Dialogs::register_always`] always gets its dialog.
    pub fn call(&self, shell: &mut Shell, id: &str, params: Value) -> Call {
        let registry = shell.engine().registry();
        let available = shell.engine().available(id, false).is_ok();
        let asks = self.always.contains(id);
        if !available || (!asks && !lacks_required(registry, id, &params)) {
            return Call::Ran(shell.run_reporting(id, params));
        }
        let dialog = match self.hand_written.get(id) {
            Some(make) => make(shell, params),
            None => Box::new(ParamForm::new(registry, id, &params)),
        };
        Call::Dialog(dialog)
    }
}

/// Whether `params` leaves out a param that `id` requires. A Preset's params are fixed, so
/// it never does.
pub fn lacks_required(registry: &Registry, id: &str, params: &Value) -> bool {
    if id.contains(':') {
        return false;
    }
    let Some(schema) = registry.params_schema(id) else {
        return false;
    };
    let given = params.as_object();
    schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|name| given.is_none_or(|g| !g.contains_key(name)))
}
