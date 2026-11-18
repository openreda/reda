//! Draws the Title bar across the top of the main window, on the base with no line
//! under it, at `title_bar_height()`. Drawing never runs anything: what was clicked comes
//! back in an [`Outcome`], for the window to run through the Shell or send to the OS.

use egui::{CornerRadius, Frame, Panel, Rect, Response, Role, Sense, Ui, UiBuilder, pos2, vec2};

use super::arrange::{Field, Widths, arrange};
use super::{Chrome, DRAG_GAP};
use crate::a11y;
use crate::bars::draw::{self, Cx, Invocation};
use crate::bars::{Action, Bars, Binding, resolve_for};
use crate::i18n;
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Glyph, Tooltip as _, WindowAction, WindowControls};

/// What the command field shows at rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldText {
    pub text: String,
    /// A prompt rather than the window's title: shown muted.
    pub hint: bool,
}

/// The window's title, "Project name — active Document", or the muted "Search
/// commands" while no Project is open.
pub fn field_text(shell: &Shell) -> FieldText {
    let Some(project) = shell.engine().project() else {
        return FieldText {
            text: tr!("command-field-no-project"),
            hint: true,
        };
    };
    let name = project.props().name.as_str();
    let text = match shell.active_document() {
        Some(document) => tr!(
            "command-field-title",
            project = name,
            document = document.title.as_str()
        ),
        None => name.to_owned(),
    };
    FieldText { text, hint: false }
}

/// What the Title bar reports from a frame.
pub struct Outcome {
    /// The menu item, quick strip button, command field or gear clicked.
    pub run: Option<Invocation>,
    /// The window control clicked, where REDA draws them.
    pub window: Option<WindowAction>,
    /// The bar's empty stretches: under everything else, sensing drags and
    /// double-clicks while the bar [`drags`](Chrome::drags) the window.
    pub background: Response,
}

/// Draws the Title bar at the top of `ui`, which fills the window.
pub fn show(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars, chrome: Chrome) -> Outcome {
    let t = ui.tokens();
    // A window REDA rounds itself shows its top corners through the bar.
    let corner = chrome.corner.round() as u8;
    let radius = CornerRadius {
        nw: corner,
        ne: corner,
        ..CornerRadius::ZERO
    };
    Panel::top("title-bar")
        .frame(Frame::NONE.fill(t.color.surface.base).corner_radius(radius))
        .show_separator_line(false)
        .resizable(false)
        .exact_size(t.title_bar_height())
        .show(ui, |ui| bar(ui, cx, bars, chrome))
        .inner
}

/// A child `Ui` laid out left to right, vertically centred, in `rect`.
fn at(ui: &mut Ui, rect: Rect) -> Ui {
    let layout = egui::Layout::left_to_right(egui::Align::Center);
    ui.new_child(UiBuilder::new().max_rect(rect).layout(layout))
}

fn bar(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars, chrome: Chrome) -> Outcome {
    let t = ui.tokens();
    let rect = ui.max_rect();
    ui.advance_cursor_after_rect(rect);
    // Allocated first, so every control on the bar is above it.
    let sense = if chrome.drags {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let background = ui.interact(rect, ui.scope_id().with("title-bar-empty"), sense);
    let muted = !ui.input(|i| i.focused);
    let side = t.control_height();
    let gap = widgets::CARD_GAP;
    let y = rect.center().y;
    let row =
        |left: f32, width: f32| Rect::from_min_size(pos2(left, y - side / 2.0), vec2(width, side));

    // The quick strip, after the platform's inset; it never collapses.
    let items = bars.toolbar.len() as f32;
    let strip_w = items * side + (items - 1.0).max(0.0) * crate::widgets::BAR_GAP;
    let strip_left = rect.left() + chrome.leading_inset.max(gap);
    let mut run = {
        let mut ui = at(ui, row(strip_left, strip_w));
        draw::quick_strip(&mut ui, cx, &bars.toolbar, side, t.icon.bar)
    };

    let titles: Vec<String> = (bars.menus.iter())
        .map(|m| i18n::translate(&m.title, &[]))
        .collect();
    let controls = if chrome.controls {
        WindowControls::width()
    } else {
        0.0
    };
    let widths = Widths {
        bar: rect.width(),
        menus_from: strip_left - rect.left() + strip_w + if items > 0.0 { gap } else { 0.0 },
        menus: widgets::menu_titles_width(ui, titles.iter().map(String::as_str)),
        button: side,
        controls,
        drag_gap: if chrome.drags { DRAG_GAP } else { 0.0 },
        gap,
    };
    let place = arrange(&widths);
    let x = |offset: f32| rect.left() + offset;

    // The menus, or the one menu button they collapse into.
    let menus_left = x(widths.menus_from);
    if place.menus_collapsed {
        let mut ui = at(ui, row(menus_left, side));
        run = run.or(collapsed_menus(&mut ui, cx, bars, muted));
    } else {
        let mut ui = at(ui, row(menus_left, widths.menus));
        run = run.or(draw::menu_bar(&mut ui, cx, bars, muted));
    }

    run = run.or(command_field(ui, cx, place.field, x, &row, muted));

    let gear = Action {
        id: "title_bar.preferences".to_owned(),
        binding: Binding::Preset("app.dialog.open:preferences".to_owned()),
        title: None,
        icon: None,
    };
    {
        let mut ui = at(ui, row(x(place.gear), side));
        run = run.or(draw::action_button(&mut ui, cx, &gear, side, t.icon.bar));
    }

    let mut window = None;
    if chrome.controls {
        let at_controls = Rect::from_min_max(pos2(rect.right() - controls, rect.top()), rect.max);
        let mut ui = at(ui, at_controls);
        // The close button's hover follows the window's corner where REDA rounds it; where
        // the OS rounds the window (Windows 11) it clips the square hover itself.
        window = widgets::window_controls(chrome.maximised)
            .height(rect.height())
            .window_focused(!muted)
            .corner(chrome.corner)
            .show(&mut ui);
    }
    Outcome {
        run,
        window,
        background,
    }
}

/// The menus as one menu button, each menu a submenu of its menu.
fn collapsed_menus(ui: &mut Ui, cx: &Cx<'_>, bars: &Bars, muted: bool) -> Option<Invocation> {
    let name = tr!("title-bar-menu");
    let response = ui.add(widgets::glyph_button(Glyph::Menu).muted(muted));
    a11y::name(&response, Role::Button, true, &name, None, None);
    let response = response.tooltip(&name);
    let mut clicked = None;
    widgets::menu(&response).show(|ui| {
        for menu in &bars.menus {
            draw::submenu(ui, cx, menu, &mut clicked);
        }
    });
    clicked
}

/// The command field, or its Icon button where the bar has no room for it. Either opens
/// the Command Palette, dropped from where the field is.
fn command_field(
    ui: &mut Ui,
    cx: &Cx<'_>,
    field: Field,
    x: impl Fn(f32) -> f32,
    row: &dyn Fn(f32, f32) -> Rect,
    muted: bool,
) -> Option<Invocation> {
    let action = Action {
        id: "title_bar.command_field".to_owned(),
        binding: Binding::Preset("app.dialog.open:palette".to_owned()),
        title: None,
        icon: None,
    };
    let item = resolve_for(cx.shell, &action, cx.document);
    let prompt = tr!("command-field-placeholder");
    let shortcut = draw::shortcut(ui, cx, &action);
    let response = match field {
        Field::Full { left, width } => {
            let shown = field_text(cx.shell);
            let mut widget = widgets::command_field(shown.text)
                .hint(shown.hint)
                .name(&prompt)
                .width(width)
                .window_focused(!muted);
            if let Some(keys) = &shortcut {
                widget = widget.shortcut(keys);
            }
            let mut ui = at(ui, row(x(left), width));
            ui.add(widget)
        }
        Field::Icon { left } => {
            let mut ui = at(ui, row(x(left), ui.tokens().control_height()));
            let response = ui.add(widgets::glyph_button(Glyph::Search).muted(muted));
            a11y::name(
                &response,
                Role::Button,
                true,
                &prompt,
                None,
                shortcut.as_deref(),
            );
            response
        }
    };
    crate::palette::remember_command_field(ui.ctx(), response.rect);
    let response = match &shortcut {
        Some(keys) => response.tooltip_key(&item.title, keys),
        None => response.tooltip(&item.title),
    };
    response.clicked().then_some(item.run.ok()).flatten()
}
