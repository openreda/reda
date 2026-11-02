//! The Keymap (docs/spec/v0.1/app.md#keymap): which key runs which Command or Preset.
//!
//! The default Keymap is a data file, `crates/reda-ui/keymap/reda.toml`, picked by the
//! Preference `keymap.base`. The user's `keymap.toml` (in the config dir) holds only
//! overrides and removals, in the same format (see [`file`]), and applies last.
//!
//! Each binding belongs to a [`Context`]. A key resolves in the most specific one that binds
//! it: the active Tool, then the active Document's type, then global. A key binds once per
//! context; a second binding of it in the same file is a clash, reported as a load error in
//! Messages, and the earlier binding is kept. Text fields and dialogs take keys before the
//! Keymap does (the window checks that before it asks).
//!
//! Bindings are [`Action`]s, the same as bar items, so they resolve, are labelled and
//! greyed out through [`bars::resolve`](crate::bars::resolve) like a menu item, and a menu
//! item shows the shortcut of the binding equal to its own.

mod chord;
mod file;

use std::fmt;

use egui::InputState;
use reda_engine::command::Registry;
use reda_engine::model::Uuid;
use reda_engine::preferences::KeymapBase;

use crate::bars::{self, Action, Binding, DocumentType};
use crate::shell::Shell;
use crate::tr;

pub use chord::{Chord, KeyName};

/// Where a binding applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Context {
    Global,
    /// While a Document of this type is the active Document.
    Document(DocumentType),
    /// While a Part's tab is the active Document. A Part has no canvas and no bars of
    /// its own, so it is not a [`DocumentType`].
    Part,
    /// While this Tool is active, such as `tool:wire`.
    Tool(String),
}

impl Context {
    /// Reads `global`, `schematic`, `pcb`, `symbol`, `footprint`, `part` or `tool:<name>`;
    /// `None` for anything else.
    pub fn parse(name: &str) -> Option<Context> {
        Some(match name {
            "global" => Context::Global,
            "schematic" => Context::Document(DocumentType::Schematic),
            "pcb" => Context::Document(DocumentType::Pcb),
            "symbol" => Context::Document(DocumentType::Symbol),
            "footprint" => Context::Document(DocumentType::Footprint),
            "part" => Context::Part,
            other => match other.strip_prefix("tool:") {
                Some(tool) if !tool.is_empty() => Context::Tool(tool.to_owned()),
                _ => return None,
            },
        })
    }
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Context::Global => f.write_str("global"),
            Context::Document(DocumentType::Schematic) => f.write_str("schematic"),
            Context::Document(DocumentType::Pcb) => f.write_str("pcb"),
            Context::Document(DocumentType::Symbol) => f.write_str("symbol"),
            Context::Document(DocumentType::Footprint) => f.write_str("footprint"),
            Context::Part => f.write_str("part"),
            Context::Tool(tool) => write!(f, "tool:{tool}"),
        }
    }
}

/// What is active now, which picks the contexts a key resolves in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Active {
    /// The active Tool's id ([`Tool::id`](crate::tools::Tool::id); `move` for the context
    /// `tool:move`).
    pub tool: Option<String>,
    /// The active Document's type.
    pub document: Option<DocumentType>,
    /// Whether the active Document is a Part's tab.
    pub part: bool,
}

impl Active {
    /// The contexts, most specific first.
    fn contexts(&self) -> impl Iterator<Item = Context> + '_ {
        let tool = self.tool.clone().map(Context::Tool);
        let document = self.document.map(Context::Document);
        let part = self.part.then_some(Context::Part);
        tool.into_iter()
            .chain(document)
            .chain(part)
            .chain(std::iter::once(Context::Global))
    }
}

/// The Commands the keys pressed this frame in `ctx`'s window bind, resolved and ready to
/// run; keys whose Command cannot run now are left out, as their greyed menu items. With a
/// dialog open (`dialog` names its undo stack) only global keys apply. A focused Panel
/// keeps the canvas's keys (Del, Mod+C, the Tools') off it, and a focused button keeps the
/// plain keys (Space presses it).
pub fn resolve_keys(
    ctx: &egui::Context,
    shell: &Shell,
    dialog: Option<reda_engine::model::Scope>,
) -> Vec<(String, serde_json::Value)> {
    let mut active = active(shell);
    let panel = matches!(shell.layout().focus, Some(crate::layout::TabKey::Panel(_)));
    if dialog.is_some() || panel {
        active = Active::default();
    }
    let widget_focused = ctx.egui_wants_keyboard_input();
    let actions = ctx.input_mut(|i| {
        let wanted = |chord: &Chord| !widget_focused || chord.passes_focused_widgets();
        shell.keymap().take_where(i, &active, wanted)
    });
    let document = shell.active_document().map(|d| d.document);
    actions
        .iter()
        .filter_map(|action| bars::resolve_in(shell, action, document, dialog).run.ok())
        .collect()
}

/// What is active in `shell`: its Tool and its active Document's type.
pub fn active(shell: &Shell) -> Active {
    Active {
        // One Tool is always active (Select at rest); its id names its context.
        tool: Some(shell.tool().id().to_owned()),
        document: bars::active_type(shell),
        part: shell
            .active_document()
            .is_some_and(|t| crate::element_editor::is_part(shell.engine(), t.document)),
    }
}

/// The shortcut of the Command or Preset `id` (`<command>:<preset>`) in what is active in
/// `shell` now, labelled as on this OS: the Command Palette's lookup.
pub fn shortcut_of(shell: &Shell, id: &str) -> Option<String> {
    let binding = if id.contains(':') {
        Binding::Preset(id.to_owned())
    } else {
        Binding::Command(id.to_owned())
    };
    let chord = shell.keymap().shortcut(&binding, &active(shell))?;
    Some(chord.label(cfg!(target_os = "macos")))
}

/// What is active for the canvas of `document` (or, with `None`, for no Document): the
/// active Tool and that Document's type. An Active Bar shows its own canvas's shortcuts.
pub fn active_for(shell: &Shell, document: Option<Uuid>) -> Active {
    let part = document.is_some_and(|uuid| crate::element_editor::is_part(shell.engine(), uuid));
    let document = document.and_then(|uuid| bars::document_type(shell, uuid));
    Active {
        document,
        part,
        ..active(shell)
    }
}

/// One key bound in one context.
#[derive(Clone, Debug, PartialEq)]
pub struct Bound {
    pub context: Context,
    pub chord: Chord,
    pub action: Action,
    /// The file it came from, as messages name it.
    pub file: &'static str,
}

/// The bindings in force: the base Keymap with the user's file laid over it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keymap {
    bindings: Vec<Bound>,
}

/// The file name messages give the user's Keymap.
pub const USER_FILE: &str = "keymap.toml";

const REDA: &str = include_str!("../keymap/reda.toml");
const REDA_FILE: &str = "keymap/reda.toml";

impl Keymap {
    /// The `base` Keymap with `user` (the text of `keymap.toml`) applied last. Returns the
    /// problems found, each a message for Messages; bad entries are skipped and a clash
    /// keeps the earlier binding.
    pub fn load(base: KeymapBase, user: Option<&str>) -> (Keymap, Vec<String>) {
        let (text, base_file) = match base {
            KeymapBase::Reda => (REDA, REDA_FILE),
        };
        let mut keymap = Keymap::default();
        let (entries, mut problems) = file::parse(base_file, text);
        keymap.apply(base_file, entries, &mut problems);
        if let Some(user) = user {
            let (entries, user_problems) = file::parse(USER_FILE, user);
            problems.extend(user_problems);
            // The user's file replaces base bindings, so clashes are within the file.
            let mut own = Keymap::default();
            own.apply(USER_FILE, entries.clone(), &mut problems);
            for entry in entries {
                match entry {
                    file::Entry::Unbind { context, chord } => keymap.remove(&context, &chord),
                    file::Entry::Bind { context, chord, .. } => {
                        let Some(bound) = own.find(&context, &chord) else {
                            continue;
                        };
                        // Only the binding the user's file kept.
                        let bound = bound.clone();
                        keymap.remove(&context, &chord);
                        if !keymap.bindings.contains(&bound) {
                            keymap.bindings.push(bound);
                        }
                    }
                }
            }
        }
        (keymap, problems)
    }

    /// Adds `entries` from `file`; a key already bound in the same context is a clash.
    fn apply(&mut self, file: &'static str, entries: Vec<file::Entry>, problems: &mut Vec<String>) {
        for entry in entries {
            match entry {
                file::Entry::Bind {
                    context,
                    chord,
                    action,
                } => {
                    if self.find(&context, &chord).is_some() {
                        problems.push(tr!(
                            "keymap-clash",
                            file = file,
                            key = chord.portable(),
                            context = context.to_string()
                        ));
                        continue;
                    }
                    self.bindings.push(Bound {
                        context,
                        chord,
                        action,
                        file,
                    });
                }
                file::Entry::Unbind { .. } if file == USER_FILE => {}
                file::Entry::Unbind { chord, .. } => problems.push(tr!(
                    "keymap-base-unbind",
                    file = file,
                    key = chord.portable()
                )),
            }
        }
    }

    fn find(&self, context: &Context, chord: &Chord) -> Option<&Bound> {
        self.bindings
            .iter()
            .find(|b| &b.context == context && &b.chord == chord)
    }

    fn remove(&mut self, context: &Context, chord: &Chord) {
        self.bindings
            .retain(|b| !(&b.context == context && &b.chord == chord));
    }

    /// Every binding in force, base bindings first.
    pub fn bindings(&self) -> &[Bound] {
        &self.bindings
    }

    /// Drops the bindings whose Command or Preset `registry` does not have, and returns a
    /// message for each.
    pub fn check(&mut self, registry: &Registry) -> Vec<String> {
        let mut problems = Vec::new();
        self.bindings.retain(|b| {
            let id = match &b.action.binding {
                Binding::Command(id) | Binding::Preset(id) => id.as_str(),
                Binding::Raw { command, .. } => command.as_str(),
            };
            let known = match id.split_once(':') {
                None => registry.get(id).is_some(),
                Some((command, name)) => registry.describe(command).is_some_and(|d| {
                    d["presets"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|p| p["id"] == id || p["id"] == name)
                }),
            };
            if !known {
                problems.push(tr!(
                    "keymap-unknown-command",
                    file = b.file,
                    key = b.chord.portable(),
                    command = id
                ));
            }
            known
        });
        problems
    }

    /// The binding `chord` runs in `active`: the one in the most specific context that
    /// binds it.
    pub fn lookup(&self, chord: &Chord, active: &Active) -> Option<&Action> {
        active
            .contexts()
            .find_map(|context| self.find(&context, chord))
            .map(|b| &b.action)
    }

    /// The key that runs `binding` in `active`, for menus and tooltips: a key whose
    /// binding a more specific context shadows does not count.
    pub fn shortcut(&self, binding: &Binding, active: &Active) -> Option<&Chord> {
        active.contexts().find_map(|context| {
            self.bindings
                .iter()
                .filter(|b| b.context == context && &b.action.binding == binding)
                .find(|b| {
                    self.lookup(&b.chord, active)
                        .is_some_and(|a| &a.binding == binding)
                })
                .map(|b| &b.chord)
        })
    }

    /// Takes the key presses this Keymap binds in `active` out of `input`, and returns
    /// their bindings in the order pressed. Other keys stay for the widgets.
    pub fn take(&self, input: &mut InputState, active: &Active) -> Vec<Action> {
        self.take_where(input, active, |_| true)
    }

    /// [`Keymap::take`], for the chords `wanted` accepts only.
    pub fn take_where(
        &self,
        input: &mut InputState,
        active: &Active,
        wanted: impl Fn(&Chord) -> bool,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        input.events.retain(|event| {
            let found = Chord::from_event(event)
                .iter()
                .filter(|chord| wanted(chord))
                .find_map(|chord| self.lookup(chord, active));
            match found {
                Some(action) => {
                    actions.push(action.clone());
                    false
                }
                None => true,
            }
        });
        actions
    }
}
