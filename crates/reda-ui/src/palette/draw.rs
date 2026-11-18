//! The Palette's area: a search field over the matching rows, in the command field's
//! place at the top of the window. It does not dim the window. Up and Down move the
//! selection, Enter or a click chooses it, Escape or a click anywhere else closes it.

use egui::text_edit::TextEditState;
use egui::{Id, Key, Order, Pos2, Rect, ScrollArea};

use super::{Row, Sources, Target};
use crate::theme::TokensExt as _;
use crate::theme::motion::Look;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// The most rows drawn at once; typing narrows the rest.
const SHOWN: usize = 200;
/// The most the list of rows takes before it scrolls.
const LIST_HEIGHT: f32 = 360.0;
/// The Palette's width when the command field is narrower.
const WIDTH: f32 = 560.0;

fn field_id() -> Id {
    Id::NULL.with("command-field-rect")
}

/// Notes where the Title bar drew the command field this frame, so the Palette opens in its
/// place.
pub fn remember_command_field(ctx: &egui::Context, rect: Rect) {
    ctx.data_mut(|d| d.insert_temp(field_id(), rect));
}

/// The open Palette: what it searches, as it was when it opened, and what has been typed.
pub struct Palette {
    sources: Sources,
    recent: Vec<Target>,
    query: String,
    selected: usize,
    /// Frames drawn so far: the click that opened it is not a click elsewhere.
    frames: u32,
    /// Put the text cursor at the end of the query on the next frame.
    cursor_to_end: bool,
}

/// What the user did with the Palette this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteOutcome {
    Open,
    Close,
    /// Go to, run or open this.
    Choose(Target),
}

impl Palette {
    /// A Palette over `sources` with `query` typed in (`">"` for Commands); `recent` are
    /// the targets chosen lately, most recent first.
    pub fn new(sources: Sources, recent: Vec<Target>, query: &str) -> Palette {
        Palette {
            sources,
            recent,
            query: query.to_owned(),
            selected: 0,
            frames: 0,
            cursor_to_end: true,
        }
    }

    /// What has been typed.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The rows it shows now.
    pub fn rows(&self) -> Vec<Row> {
        self.sources.rows(&self.query, &self.recent)
    }

    pub fn show(&mut self, ctx: &egui::Context) -> PaletteOutcome {
        let found = self.rows();
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            return PaletteOutcome::Close;
        }
        let last = found.len().min(SHOWN).saturating_sub(1);
        if down {
            self.selected = (self.selected + 1).min(last);
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        self.selected = self.selected.min(last);
        let mut chosen = None;
        if enter
            && let Some(row) = found.get(self.selected)
            && row.unavailable.is_none()
        {
            chosen = Some(row.target.clone());
        }

        let id = Id::NULL.with("command-palette");
        let screen = ctx.content_rect();
        let field = ctx.data(|d| d.get_temp::<Rect>(field_id()));
        let width = field
            .map_or(WIDTH, |f| f.width().max(WIDTH))
            .min(screen.width() - 16.0);
        let centre = field.map_or(screen.center().x, |f| f.center().x);
        let left = (centre - width / 2.0)
            .min(screen.right() - 8.0 - width)
            .max(screen.left() + 8.0);
        // The Palette's field sits in the command field's place: the frame starts its
        // padding above it.
        let inset = widgets::palette_frame_inset(&ctx.tokens());
        let top = field.map_or(screen.top() + 4.0, |f| f.top() - inset);
        let t = ctx.tokens();
        let area = egui::Area::new(id)
            .order(Order::Foreground)
            .fixed_pos(Pos2::new(left, top))
            .fade_in(false)
            .show(ctx, |ui| {
                widgets::palette_frame(&t).show(ui, |ui| {
                    ui.set_width(width - 2.0 * widgets::palette_frame_inset(&t));
                    let edit = widgets::text_field(&mut self.query)
                        .search()
                        .id(id.with("query"))
                        .hint(tr!("palette-hint"))
                        .width(f32::INFINITY);
                    let field = ui.add(edit);
                    field.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr!("palette-title"))
                    });
                    if self.frames == 0 {
                        field.request_focus();
                    }
                    if std::mem::take(&mut self.cursor_to_end) {
                        let mut state = TextEditState::load(ui.ctx(), field.id).unwrap_or_default();
                        let end = egui::text::CCursor::new(self.query.chars().count());
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(end)));
                        state.store(ui.ctx(), field.id);
                    }
                    if field.changed() {
                        self.selected = 0;
                    }
                    ui.add_space(t.space.row_y);
                    if found.is_empty() {
                        ui.label(
                            t.ty.body
                                .rich(tr!("palette-nothing"))
                                .color(t.color.text.muted),
                        );
                    }
                    // The Area remembers its last size: without a minimum, a list that was short keeps
                    // its short height when it grows again (#281).
                    ScrollArea::vertical()
                        .max_height(LIST_HEIGHT)
                        .min_scrolled_height(LIST_HEIGHT)
                        .show(ui, |ui| {
                            widgets::rows(ui, |ui| {
                                for (i, row) in found.iter().take(SHOWN).enumerate() {
                                    let (target, response) = draw_row(ui, row, i == self.selected);
                                    if target.is_some() {
                                        chosen = target;
                                    }
                                    if i == self.selected && (up || down) {
                                        response.scroll_to_me(None);
                                    }
                                }
                            });
                        });
                });
            });
        // It fades in dropping from the command field, and fades out when it closes.
        let motion = ctx.tokens().motion;
        let look = Look::travel(egui::vec2(0.0, -motion.travel));
        crate::theme::motion::layer(ctx, area.response.layer_id, look, motion.palette);
        let rect = area.response.rect;
        let elsewhere = self.frames > 0
            && ctx.input(|i| {
                i.pointer.any_pressed()
                    && i.pointer.interact_pos().is_some_and(|p| !rect.contains(p))
            });
        self.frames += 1;
        match chosen {
            Some(Target::Prefix(scope)) => {
                self.query = scope.prefix().to_owned();
                self.selected = 0;
                self.cursor_to_end = true;
                ctx.memory_mut(|m| m.request_focus(id.with("query")));
                PaletteOutcome::Open
            }
            Some(target) => PaletteOutcome::Choose(target),
            None if elsewhere => PaletteOutcome::Close,
            None => PaletteOutcome::Open,
        }
    }
}

/// One row; returns its target when clicked.
fn draw_row(ui: &mut egui::Ui, row: &Row, selected: bool) -> (Option<Target>, egui::Response) {
    let enabled = row.unavailable.is_none();
    // The menu path, the shortcut (a Command's detail) or other detail, then why it
    // cannot run now.
    let mut shown = widgets::palette_row(row.title.as_str())
        .icon(&row.icon)
        .selected(selected);
    if let Some(path) = &row.menu_path {
        shown = shown.note(path);
    }
    if let Some(detail) = &row.detail {
        shown = match row.target {
            Target::Run(_) => shown.shortcut(detail),
            _ => shown.note(detail),
        };
    }
    if let Some(why) = &row.unavailable {
        shown = shown.note(why);
    }
    if row.recent {
        shown = shown.note(tr!("palette-recent"));
    }
    let response = ui.add_enabled(enabled, shown);
    let response = if row.description.is_empty() {
        response
    } else {
        response.tooltip(&row.description)
    };
    let target = response.clicked().then(|| row.target.clone());
    (target, response)
}
