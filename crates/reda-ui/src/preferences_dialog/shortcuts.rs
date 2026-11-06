//! The Keyboard Shortcuts page (docs/spec/v0.1/app.md#keymap): every Command and Preset
//! with the keys that run it, search, chord recording with conflict flags, removing a key,
//! reset to default and an "Open folder" link.
//!
//! Changes go to the user's `keymap.toml` as `[[bind]]` and `[[unbind]]` entries (edited
//! in place, so comments survive) and the Shell reloads the Keymap at once. Clashes in
//! the file show as the Keymap reports them: as load errors, here and in Messages.

use std::collections::BTreeSet;
use std::path::PathBuf;

use reda_engine::command::Kind;

use super::user_keymap;
use crate::bars::{self, Action, Binding};
use crate::keymap::{self, Bound, Chord, Context, Keymap};
use crate::shell::Shell;
use crate::tr;

/// One Command or Preset the page lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub binding: Binding,
    /// The Command or Preset id (a raw binding's Command).
    pub id: String,
    pub title: String,
}

/// A key being recorded for a binding.
#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    pub binding: Binding,
    /// Where the key will apply.
    pub context: Context,
    /// The key pressed so far; `None` while waiting for one.
    pub chord: Option<Chord>,
}

/// The page's state: the search, the key being recorded and the last error.
#[derive(Clone, Debug, Default)]
pub struct Shortcuts {
    pub query: String,
    pub recording: Option<Recording>,
    rows: Option<Vec<Row>>,
    error: Option<String>,
}

impl Shortcuts {
    /// Every Command (that is not a query and needs no params) and Preset, plus every raw
    /// binding a Keymap has, sorted by title. Built once per dialog.
    pub fn rows(&mut self, shell: &Shell) -> &[Row] {
        self.rows.get_or_insert_with(|| rows(shell))
    }

    /// The rows whose title or id contains `query`, or one of whose keys is `query`
    /// (ignoring case). An empty query lists every row.
    pub fn matching(&mut self, shell: &Shell, query: &str) -> Vec<Row> {
        let query = query.trim().to_lowercase();
        let mac = cfg!(target_os = "macos");
        let rows = self.rows(shell).to_vec();
        rows.into_iter()
            .filter(|row| {
                query.is_empty()
                    || row.title.to_lowercase().contains(&query)
                    || row.id.to_lowercase().contains(&query)
                    || Shortcuts::keys(shell, &row.binding).iter().any(|(_, c)| {
                        c.portable().to_lowercase() == query || c.label(mac).to_lowercase() == query
                    })
            })
            .collect()
    }

    /// The keys that run `binding` now, in each context, in Keymap order.
    pub fn keys(shell: &Shell, binding: &Binding) -> Vec<(Context, Chord)> {
        keys_in(shell.keymap(), binding)
    }

    /// Whether `binding`'s keys differ from the base Keymap's.
    pub fn is_changed(shell: &Shell, binding: &Binding) -> bool {
        let now: BTreeSet<String> = keys_in(shell.keymap(), binding)
            .iter()
            .map(|(c, k)| format!("{c} {k}"))
            .collect();
        let base: BTreeSet<String> = keys_in(&base(shell), binding)
            .iter()
            .map(|(c, k)| format!("{c} {k}"))
            .collect();
        now != base
    }

    /// The load errors of the Keymap files (clashes, unknown Commands, bad entries), as
    /// Messages lists them.
    pub fn problems(shell: &Shell) -> Vec<String> {
        shell
            .messages()
            .notes()
            .iter()
            .filter(|n| n.source == "keymap")
            .map(|n| n.text.clone())
            .collect()
    }

    /// The folder `keymap.toml` lives in, for "Open folder".
    pub fn folder(shell: &Shell) -> Option<PathBuf> {
        shell.engine().user_dirs().map(|d| d.config.clone())
    }

    /// The last change that failed, if it did.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Starts recording a key for `binding`, in the context its keys already use (else
    /// global).
    pub fn start_recording(&mut self, binding: Binding) {
        self.recording = Some(Recording {
            binding,
            context: Context::Global,
            chord: None,
        });
    }

    /// Starts recording, in the context of `binding`'s first key in `shell`.
    pub fn start_recording_in(&mut self, shell: &Shell, binding: Binding) {
        let context = Shortcuts::keys(shell, &binding)
            .into_iter()
            .next()
            .map_or(Context::Global, |(c, _)| c);
        self.recording = Some(Recording {
            binding,
            context,
            chord: None,
        });
    }

    /// Where the key being recorded will apply.
    pub fn set_context(&mut self, context: Context) {
        if let Some(r) = &mut self.recording {
            r.context = context;
        }
    }

    /// Takes the key pressed.
    pub fn record(&mut self, chord: Chord) {
        if let Some(r) = &mut self.recording {
            r.chord = Some(chord);
        }
    }

    pub fn cancel(&mut self) {
        self.recording = None;
    }

    /// What the recorded key runs now in its context, when that is something else: the
    /// conflict confirming would replace.
    pub fn conflict(&self, shell: &Shell) -> Option<String> {
        let r = self.recording.as_ref()?;
        let chord = r.chord.as_ref()?;
        let other = find(shell.keymap(), &r.context, chord)?;
        if other.action.binding == r.binding {
            return None;
        }
        Some(tr!(
            "shortcuts-conflict",
            key = chord.portable(),
            command = title(shell, &other.action.binding),
            context = r.context.to_string()
        ))
    }

    /// Binds the recorded key, replacing what it ran in that context.
    pub fn confirm(&mut self, shell: &mut Shell) -> Result<(), String> {
        let Some(Recording {
            binding,
            context,
            chord: Some(chord),
        }) = self.recording.clone()
        else {
            return Err(tr!("shortcuts-no-key"));
        };
        let base = base(shell);
        let out = edit(shell, |doc| {
            user_keymap::remove(doc, &context, &chord);
            let default = find(&base, &context, &chord).map(|b| &b.action.binding);
            if default != Some(&binding) {
                user_keymap::add_bind(doc, &context, &chord, &binding);
            }
        });
        self.done(out)
    }

    /// Removes `chord` in `context`: the user's own binding goes, and a default one is
    /// unbound.
    pub fn unbind(
        &self,
        shell: &mut Shell,
        context: &Context,
        chord: &Chord,
    ) -> Result<(), String> {
        let base = base(shell);
        edit(shell, |doc| {
            user_keymap::remove(doc, context, chord);
            if find(&base, context, chord).is_some() {
                user_keymap::add_unbind(doc, context, chord);
            }
        })
    }

    /// [`Shortcuts::unbind`], keeping a failure as the page's error.
    pub fn remove_key(&mut self, shell: &mut Shell, context: &Context, chord: &Chord) {
        let out = self.unbind(shell, context, chord);
        let _ = self.done(out);
    }

    /// Puts `binding`'s keys back to the default: drops the user's entries for every key
    /// it has now or has by default.
    pub fn reset(&self, shell: &mut Shell, binding: &Binding) -> Result<(), String> {
        let mut keys = keys_in(shell.keymap(), binding);
        keys.extend(keys_in(&base(shell), binding));
        edit(shell, |doc| {
            for (context, chord) in &keys {
                user_keymap::remove(doc, context, chord);
            }
        })
    }

    /// [`Shortcuts::reset`], keeping a failure as the page's error.
    pub fn reset_binding(&mut self, shell: &mut Shell, binding: &Binding) {
        let out = self.reset(shell, binding);
        let _ = self.done(out);
    }

    fn done(&mut self, out: Result<(), String>) -> Result<(), String> {
        match &out {
            Ok(()) => {
                self.recording = None;
                self.error = None;
            }
            Err(e) => self.error = Some(e.clone()),
        }
        out
    }
}

/// Edits the user's `keymap.toml` and reloads the Keymap.
fn edit(shell: &mut Shell, change: impl FnOnce(&mut toml_edit::DocumentMut)) -> Result<(), String> {
    let Some(dir) = Shortcuts::folder(shell) else {
        return Err(tr!("shortcuts-no-folder"));
    };
    let path = dir.join(keymap::USER_FILE);
    let failed = |e: std::io::Error| {
        tr!(
            "shortcuts-write-failed",
            file = keymap::USER_FILE,
            error = e.to_string()
        )
    };
    let mut doc = user_keymap::read(&path).map_err(failed)?;
    change(&mut doc);
    user_keymap::write(&path, &doc).map_err(failed)?;
    shell.reload_keymap();
    Ok(())
}

/// The base Keymap the Preferences pick, without the user's file.
fn base(shell: &Shell) -> Keymap {
    Keymap::load(shell.preferences().keymap.base, None).0
}

fn find<'k>(keymap: &'k Keymap, context: &Context, chord: &Chord) -> Option<&'k Bound> {
    keymap
        .bindings()
        .iter()
        .find(|b| &b.context == context && &b.chord == chord)
}

fn keys_in(keymap: &Keymap, binding: &Binding) -> Vec<(Context, Chord)> {
    keymap
        .bindings()
        .iter()
        .filter(|b| &b.action.binding == binding)
        .map(|b| (b.context.clone(), b.chord))
        .collect()
}

/// The title a binding shows, as a menu item would.
pub(super) fn title(shell: &Shell, binding: &Binding) -> String {
    // A Preset reads with its full title ("Theme: Dark"), not the shorter one its menu
    // item has, since the page lists it on its own.
    if let Binding::Preset(id) = binding
        && let Some((command, name)) = id.split_once(':')
        && let Some(described) = shell.engine().registry().describe(command)
        && let Some(title) = described["presets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == id.as_str() || p["id"] == name)
            .and_then(|p| p["title"].as_str())
    {
        return title.to_owned();
    }
    let action = Action {
        id: String::new(),
        binding: binding.clone(),
        title: None,
        icon: None,
    };
    bars::resolve(shell, &action).title
}

fn rows(shell: &Shell) -> Vec<Row> {
    let registry = shell.engine().registry();
    let mut bindings = Vec::new();
    for meta in registry.commands() {
        if meta.kind == Kind::Query {
            continue;
        }
        let described = registry.describe(meta.id).unwrap_or_default();
        let needs_params = described["params"]["required"]
            .as_array()
            .is_some_and(|r| !r.is_empty());
        if !needs_params {
            bindings.push(Binding::Command(meta.id.to_owned()));
        }
        for preset in described["presets"].as_array().into_iter().flatten() {
            let Some(name) = preset["id"].as_str() else {
                continue;
            };
            let name = name.rsplit(':').next().unwrap_or(name);
            bindings.push(Binding::Preset(format!("{}:{name}", meta.id)));
        }
    }
    for keymap in [shell.keymap(), &base(shell)] {
        for bound in keymap.bindings() {
            if !bindings.contains(&bound.action.binding) {
                bindings.push(bound.action.binding.clone());
            }
        }
    }
    let mut rows: Vec<Row> = bindings
        .into_iter()
        .map(|binding| {
            let id = match &binding {
                Binding::Command(id) | Binding::Preset(id) => id.clone(),
                Binding::Raw { command, .. } => command.clone(),
            };
            Row {
                title: title(shell, &binding),
                id,
                binding,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)));
    rows
}
