//! The Palette's window: a search field over the matching entries, near the top of the
//! window. Up and Down move the selection, Enter or a click runs it, Escape or a click
//! outside closes the Palette.

use egui::{Align2, Button, Id, Key, RichText, ScrollArea, TextEdit};

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
        let query = crate::proto_shell::with(|s| s.palette_query.take());
        Palette {
            entries,
            // Mod+Shift+P opens on Commands; the field and Mod+P open on go-to.
            query: query.unwrap_or_else(|| ">".to_owned()),
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
        let theme = ctx.theme();
        let mut suggestion = None;
        // Over the top row's field, with nothing dimmed, as VS Code's.
        let area = egui::Area::new(id)
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, [0.0, 4.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(620.0);
                    let mut output = TextEdit::singleline(&mut self.query)
                        .id(id.with("query"))
                        .hint_text("Search Documents, Components, Nets. > Commands. ? Help")
                        .desired_width(f32::INFINITY)
                        .show(ui);
                    let field = output.response.clone();
                    if !self.focused {
                        field.request_focus();
                        let end = egui::text::CCursor::new(self.query.chars().count());
                        output
                            .state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(end)));
                        output.state.store(ui.ctx(), field.id);
                        self.focused = true;
                    }
                    if field.changed() {
                        self.selected = 0;
                    }
                    if self.query.is_empty() || self.query == "?" {
                        for (prefix, label) in [
                            (">", "Show and run Commands"),
                            ("@", "Go to a Component"),
                            ("#", "Highlight a Net"),
                            (":", "Open a Document"),
                        ] {
                            let row = Button::new(label)
                                .frame_when_inactive(false)
                                .right_text(RichText::new(prefix).weak())
                                .min_size(egui::vec2(ui.available_width(), 0.0));
                            if ui.add(row).clicked() {
                                suggestion = Some(prefix);
                            }
                        }
                        ui.separator();
                    }
                    ui.add_space(4.0);
                    if found.is_empty() {
                        ui.weak(tr!("palette-nothing"));
                    }
                    ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for (i, entry) in found.iter().take(SHOWN).enumerate() {
                            let enabled = entry.unavailable.is_none();
                            let mut button =
                                match icons::image(&entry.icon, icons::TAB, theme, enabled) {
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
            });
        if let Some(prefix) = suggestion {
            self.query = prefix.to_owned();
            self.focused = false;
            self.selected = 0;
        }
        let outside = ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !area.response.rect.contains(p))
        });
        let escape = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
        if (outside || escape) && outcome == PaletteOutcome::Open {
            outcome = PaletteOutcome::Close;
        }
        outcome
    }
}
