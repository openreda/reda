//! The Palette's window: a search field over the matching entries, near the top of the
//! window. Up and Down move the selection, Enter or a click runs it, Escape or a click
//! outside closes the Palette.

use egui::{Align2, Button, Id, Key, Modal, RichText, ScrollArea, TextEdit};

use super::{Entry, search};
use crate::{icons, tr};

/// The most entries drawn at once; typing narrows the rest.
const SHOWN: usize = 200;

/// The open Palette: its entries, as they were when it opened, and what has been typed.
pub struct Palette {
    entries: Vec<Entry>,
    query: String,
    selected: usize,
    focused: bool,
}

/// What the user did with the Palette this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteOutcome {
    Open,
    Close,
    /// Run this Command or Preset id.
    Run(String),
}

impl Palette {
    pub fn new(entries: Vec<Entry>) -> Palette {
        Palette {
            entries,
            query: String::new(),
            selected: 0,
            focused: false,
        }
    }

    /// What has been typed.
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn show(&mut self, ctx: &egui::Context) -> PaletteOutcome {
        let found = search(&self.entries, &self.query);
        let (up, down, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
            )
        });
        let last = found.len().min(SHOWN).saturating_sub(1);
        if down {
            self.selected = (self.selected + 1).min(last);
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        self.selected = self.selected.min(last);
        let mut outcome = PaletteOutcome::Open;
        if enter
            && let Some(entry) = found.get(self.selected)
            && entry.unavailable.is_none()
        {
            outcome = PaletteOutcome::Run(entry.id.clone());
        }

        let id = Id::NULL.with("command-palette");
        let area = Modal::default_area(id).anchor(Align2::CENTER_TOP, [0.0, 48.0]);
        let theme = ctx.theme();
        let modal = Modal::new(id).area(area).show(ctx, |ui| {
            ui.set_width(620.0);
            let field = ui.add(
                TextEdit::singleline(&mut self.query)
                    .id(id.with("query"))
                    .hint_text(tr!("palette-hint"))
                    .desired_width(f32::INFINITY),
            );
            field.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr!("palette-title"))
            });
            if !self.focused {
                field.request_focus();
                self.focused = true;
            }
            if field.changed() {
                self.selected = 0;
            }
            ui.weak("@ Component   # Net   : Document   Right-click a row: Change shortcut");
            ui.add_space(4.0);
            if found.is_empty() {
                ui.weak(tr!("palette-nothing"));
            }
            ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (i, entry) in found.iter().take(SHOWN).enumerate() {
                    let enabled = entry.unavailable.is_none();
                    let mut button = match icons::image(&entry.icon, icons::TAB, theme, enabled) {
                        Some(image) => Button::new((image, entry.title.as_str())),
                        None => Button::new(entry.title.as_str()),
                    }
                    .selected(i == self.selected)
                    .frame_when_inactive(i == self.selected)
                    .min_size(egui::vec2(ui.available_width(), 0.0));
                    let right = match (&entry.unavailable, &entry.shortcut) {
                        (Some(reason), _) => Some(reason.as_str()),
                        (None, Some(shortcut)) => Some(shortcut.as_str()),
                        (None, None) => None,
                    };
                    let path = entry.path.as_deref().unwrap_or_default();
                    let right = match right {
                        Some(right) if !path.is_empty() => format!("{path}    {right}"),
                        Some(right) => right.to_owned(),
                        None => path.to_owned(),
                    };
                    if !right.is_empty() {
                        button = button.right_text(RichText::new(right).weak());
                    }
                    let response = ui.add_enabled(enabled, button);
                    if i == self.selected && (up || down) {
                        response.scroll_to_me(None);
                    }
                    let response = if entry.description.is_empty() {
                        response
                    } else {
                        response.on_hover_text(&entry.description)
                    };
                    response.context_menu(|ui| {
                        let _ = ui.button("Change shortcut…");
                    });
                    if response.clicked() {
                        outcome = PaletteOutcome::Run(entry.id.clone());
                    }
                }
            });
        });
        if modal.should_close() && outcome == PaletteOutcome::Open {
            outcome = PaletteOutcome::Close;
        }
        outcome
    }
}
