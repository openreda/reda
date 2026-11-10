//! Keys in Tool (docs/spec/v0.1/app.md#keys-in-tool): Shift+F1 toggles a card at the
//! canvas's top right listing the keys valid in the active Tool, generated from the
//! Keymap: the Tool's own bindings, then its Document type's that the Tool leaves in
//! force. Two columns of chord and action, titled for the Tool; it shows no prompt and
//! takes no clicks.

use egui::{Align2, Id, Order, Rect, Role, Ui, WidgetInfo};
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use schemars::JsonSchema;
use serde::Serialize;

use crate::bars::{self, Action, Binding};
use crate::keymap::{self, Context as KeyContext};
use crate::shell::Shell;
use crate::tr;

/// Whether the card shows: view state, toggled by `view.keys_card.toggle`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeysCard {
    pub shown: bool,
}

/// Adds `view.keys_card.toggle` to `registry`. The Shell does this.
pub fn register_all(registry: &mut Registry) {
    registry.add(Toggle);
}

#[derive(Serialize, JsonSchema)]
pub struct ToggleResult {
    /// Whether the card now shows.
    pub shown: bool,
}

struct Toggle;

impl Command for Toggle {
    const META: Meta = Meta {
        id: "view.keys_card.toggle",
        title: "Keys in Tool",
        description: "Shows or hides the card of the keys valid in the active Tool, \
                      generated from the Keymap, at the canvas's top right. View state only.",
        category: "view",
        icon: "select",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = ToggleResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<ToggleResult, CommandError> {
        let card = cx.view::<KeysCard>()?;
        card.shown = !card.shown;
        Ok(ToggleResult { shown: card.shown })
    }
}

/// What the card shows: its title and one `(chord, action)` row per key.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

/// The card for the active Tool on the active Document, labelled as on macOS (`mac`) or
/// elsewhere: the Tool's own bindings in Keymap order, then its Document type's whose
/// chord the Tool does not take over.
pub fn card(shell: &Shell, mac: bool) -> Card {
    let active = keymap::active(shell);
    let tool = shell.tool();
    let document = active.document.map(KeyContext::Document);
    let own = KeyContext::Tool(tool.id().to_owned());
    let bindings = shell.keymap().bindings();
    let mut rows = Vec::new();
    let mut taken = Vec::new();
    let mut add = |b: &keymap::Bound, rows: &mut Vec<(String, String)>| {
        if taken.contains(&b.chord) {
            return;
        }
        taken.push(b.chord);
        let title = bars::resolve(shell, &b.action).title;
        rows.push((b.chord.label(mac), title));
    };
    for context in std::iter::once(own).chain(document) {
        for b in bindings.iter().filter(|b| b.context == context) {
            add(b, &mut rows);
        }
    }
    let activate = Action {
        id: String::new(),
        binding: Binding::Preset(format!("tool.activate:{}", tool.id())),
        title: None,
        icon: None,
    };
    let tool_title = bars::resolve(shell, &activate).title;
    Card {
        title: tr!("keys-card-title", tool = tool_title),
        rows,
    }
}

/// Draws the card at the top right of `canvas` while it is shown. It takes no clicks.
pub fn show(ui: &Ui, shell: &Shell, canvas: Rect) {
    if !shell
        .engine()
        .view()
        .get::<KeysCard>()
        .is_some_and(|c| c.shown)
    {
        return;
    }
    let card = card(shell, ui.ctx().os().is_mac());
    let id = Id::NULL.with(("keys-card", canvas.min.x.to_bits(), canvas.min.y.to_bits()));
    egui::Area::new(id)
        .order(Order::Foreground)
        .interactable(false)
        .fixed_pos(canvas.right_top() + egui::vec2(-12.0, 12.0))
        .pivot(Align2::RIGHT_TOP)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let heading = ui.strong(card.title.as_str());
                let title = card.title.clone();
                heading.widget_info(|| WidgetInfo::labeled(Role::Label, true, title.clone()));
                egui::Grid::new(id.with("rows"))
                    .num_columns(2)
                    .spacing([16.0, 2.0])
                    .show(ui, |ui| {
                        for (chord, action) in &card.rows {
                            ui.monospace(chord.as_str());
                            ui.label(action.as_str());
                            ui.end_row();
                        }
                    });
            });
        });
}
