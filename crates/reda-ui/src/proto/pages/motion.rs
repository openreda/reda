//! PROTOTYPE (issue #282): the Motion page. Throwaway.
//!
//! Every transition the chrome could animate, live, in three profiles (`anim::PROFILES`,
//! F8 cycles): click things. The strip at the top picks the profile, turns Animate off,
//! slows everything down, and plays every demo at once. The card at the right lists what
//! the current profile does, so the state is always in view.

use egui::emath::TSTransform;
use egui::{
    Align, Align2, Color32, Id, LayerId, Layout, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    UiBuilder, Vec2, pos2, vec2,
};

use super::super::Tokens;
use super::super::anim::{self, Profile, Tr};
use super::super::widgets::{self as w, Check, Field, Glyph, Item, Kind, Sel, St, Tab, TabKind};
use super::{column, note, section};

/// The inside width of a demo card.
const W: f32 = 348.0;
/// The height of a mock window.
const WIN_H: f32 = 184.0;

#[derive(Clone)]
struct Demo {
    toggle: bool,
    check: bool,
    radio: usize,
    switch: bool,
    text: String,
    tab: usize,
    menu: bool,
    combo: bool,
    combo_value: usize,
    row: usize,
    sections: [bool; 2],
    dock: bool,
    flyout: bool,
    dialog: bool,
    palette: bool,
    tool: usize,
    progress: f32,
}

impl Default for Demo {
    fn default() -> Self {
        Demo {
            toggle: false,
            check: false,
            radio: 0,
            switch: false,
            text: String::new(),
            tab: 0,
            menu: false,
            combo: false,
            combo_value: 0,
            row: 1,
            sections: [true, false],
            dock: true,
            flyout: false,
            dialog: false,
            palette: false,
            tool: 0,
            progress: 0.25,
        }
    }
}

impl Demo {
    /// Play all: every demo to its other state at once.
    fn play(&mut self, on: bool) {
        self.toggle = on;
        self.check = on;
        self.radio = usize::from(on);
        self.switch = on;
        self.tab = if on { 2 } else { 0 };
        self.menu = on;
        self.row = if on { 4 } else { 1 };
        self.sections = [!on, on];
        self.dock = !on;
        self.flyout = on;
        self.dialog = on;
        self.palette = on;
        self.tool = if on { 2 } else { 0 };
        self.progress = if on { 1.0 } else { 0.25 };
    }
}

fn id(name: &str) -> Id {
    Id::new(("proto-motion", name))
}

pub fn page(ui: &mut Ui, t: &Tokens) {
    let p = anim::profile();
    let key = id("demo");
    let mut d = ui.data(|m| m.get_temp::<Demo>(key)).unwrap_or_default();
    if let Some(on) = anim::take_play() {
        d.play(on);
    }
    strip(ui, t);
    ui.add_space(4.0);
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(t.space.gap, t.space.stack);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.horizontal_top(|ui| {
                    card(ui, t, "Controls and progress", |ui| controls(ui, t, &p, &mut d));
                    card(ui, t, "Tabs and tooltips", |ui| {
                        tabs(ui, t, &p, &mut d);
                        ui.add_space(4.0);
                        tooltips(ui, t, &p);
                    });
                    card(ui, t, "On the canvas: Active Bar and a Tool's bar", |ui| {
                        active_bar(ui, t, &p, &mut d);
                    });
                });
                ui.horizontal_top(|ui| {
                    card(ui, t, "Menu and combo list", |ui| menus(ui, t, &p, &mut d));
                    card(ui, t, "Selected row and scrollbar", |ui| list(ui, t, &p, &mut d));
                    card(ui, t, "Panel sections", |ui| sections(ui, t, &p, &mut d));
                });
                ui.horizontal_top(|ui| {
                    card(ui, t, "Dock collapse and flyout", |ui| dock(ui, t, &p, &mut d));
                    card(ui, t, "Dialog and scrim", |ui| dialog(ui, t, &p, &mut d));
                    card(ui, t, "Command Palette", |ui| palette(ui, t, &p, &mut d));
                });
            });
            column(ui, 404.0, |ui| {
                section(ui, t, &format!("What {} does", p.name), |ui| {
                    ui.set_width(404.0 - 2.0 * t.space.panel - 4.0);
                    ui.set_max_width(404.0 - 2.0 * t.space.panel - 4.0);
                    timings(ui, t, &p);
                });
                section(ui, t, "Never animated, in any profile", |ui| {
                    ui.set_width(404.0 - 2.0 * t.space.panel - 4.0);
                    ui.set_max_width(404.0 - 2.0 * t.space.panel - 4.0);
                    never(ui, t);
                });
            });
        });
        ui.add_space(48.0);
    });
    ui.data_mut(|m| m.insert_temp(key, d));
}

fn card(ui: &mut Ui, t: &Tokens, title: &str, add: impl FnOnce(&mut Ui)) {
    column(ui, W + 2.0 * t.space.panel, |ui| {
        section(ui, t, title, |ui| {
            ui.set_width(W);
            add(ui);
        });
    });
}

/// Profile, Animate, speed, Play all.
fn strip(ui: &mut Ui, t: &Tokens) {
    let pick = |on: bool| if on { Kind::On } else { Kind::Secondary };
    ui.horizontal(|ui| {
        for (i, profile) in anim::PROFILES.iter().enumerate() {
            let kind = pick(i == anim::profile_index());
            if w::button(ui, t, St::Live, kind, None, profile.name).clicked() {
                anim::set_profile(i);
            }
        }
        ui.add_space(12.0);
        if w::switch(ui, t, St::Live, anim::animate(), "Animate").clicked() {
            anim::set_animate(!anim::animate());
        }
        ui.add_space(12.0);
        w::label(ui, &t.ty.body, t.color.text.muted, "Speed");
        for (i, (name, _)) in anim::SPEEDS.iter().enumerate() {
            let kind = pick(i == anim::speed_index());
            if w::button(ui, t, St::Live, kind, None, name).clicked() {
                anim::set_speed(i);
            }
        }
        ui.add_space(12.0);
        if w::button(ui, t, St::Live, Kind::Primary, None, "Play all").clicked() {
            anim::play();
        }
    });
    let about = anim::PROFILES[anim::profile_index()].about;
    w::label(ui, &t.ty.body, t.color.text.muted, about);
}

// --- Helpers --------------------------------------------------------------------------

/// A child of `ui` in a layer above the page, for what floats over other cards.
fn overlay<R>(ui: &mut Ui, name: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let layer = LayerId::new(Order::Foreground, id(name));
    let builder = UiBuilder::new()
        .layer_id(layer)
        .max_rect(ui.ctx().content_rect());
    let mut child = ui.new_child(builder);
    add(&mut child)
}

/// A child of `ui` that paints at `opacity`, clipped to `clip`, and takes no space.
fn faded<R>(ui: &mut Ui, opacity: f32, clip: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(UiBuilder::new().max_rect(clip));
    child.set_clip_rect(clip.intersect(ui.clip_rect()));
    child.multiply_opacity(opacity.clamp(0.0, 1.0));
    add(&mut child)
}

/// Something arriving: `k` is its presence (0 gone, 1 there). It fades in, travels the
/// last `travel` pixels, and grows from `scale` about `pivot`.
fn present(
    ui: &mut Ui,
    k: f32,
    pivot: Pos2,
    travel: Vec2,
    scale: f32,
    add: impl FnOnce(&mut Ui),
) {
    let s = egui::lerp(scale..=1.0, k);
    let transform = TSTransform::new(pivot.to_vec2() * (1.0 - s) + travel * (1.0 - k), s);
    let clip = ui.clip_rect();
    let mut child = ui.new_child(UiBuilder::new().max_rect(clip));
    child.with_visual_transform(transform, |ui| {
        ui.multiply_opacity(k.clamp(0.0, 1.0));
        add(ui);
    });
}

/// A child laid out in `rect`.
fn within<R>(ui: &mut Ui, rect: Rect, layout: Layout, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(layout));
    add(&mut child)
}

/// A mock window: the base colour, rounded, with a rim.
fn mini(ui: &mut Ui, t: &Tokens, height: f32) -> Rect {
    let (rect, _) = ui.allocate_exact_size(vec2(W, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, t.radius.card, t.color.surface.base);
    ui.painter().rect_stroke(
        rect,
        t.radius.card,
        Stroke::new(t.hairline, t.color.stroke.hairline),
        StrokeKind::Inside,
    );
    rect
}

fn key_chip(ui: &Ui, t: &Tokens, right_center: Pos2, keys: &str) -> f32 {
    let width = w::width(ui, &t.ty.small, keys) + 10.0;
    let height = t.ty.small.line + 2.0;
    let rect = Rect::from_min_size(
        pos2(right_center.x - width, right_center.y - height / 2.0),
        vec2(width, height),
    );
    ui.painter()
        .rect_filled(rect, t.radius.mark, t.color.fill.control);
    let ink = t.color.text.muted;
    w::text_in(ui, rect, Align2::CENTER_CENTER, &t.ty.small, keys, ink);
    width
}

/// A chevron that turns from right (`k` = 0) to down (`k` = 1).
fn chevron(ui: &Ui, c: Pos2, k: f32, color: Color32) {
    let a = k * std::f32::consts::FRAC_PI_2;
    let turn = |x: f32, y: f32| c + vec2(x * a.cos() - y * a.sin(), x * a.sin() + y * a.cos());
    let pen = Stroke::new(1.5, color);
    let s = 4.5;
    ui.painter()
        .line_segment([turn(-0.3 * s, -0.6 * s), turn(0.3 * s, 0.0)], pen);
    ui.painter()
        .line_segment([turn(0.3 * s, 0.0), turn(-0.3 * s, 0.6 * s)], pen);
}

/// The pointer went down somewhere outside all of `keep`.
fn pressed_outside(ui: &Ui, keep: &[Rect]) -> bool {
    ui.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|at| !keep.iter().any(|r| r.contains(at)))
    })
}

const LAYERS: [(&str, u32); 5] = [
    ("Top", 0xc83434),
    ("Inner 1", 0x7fc87f),
    ("Inner 2", 0xce7d2c),
    ("Bottom", 0x4d7fc4),
    ("Outline", 0xd0d2cd),
];

fn layer_rows(ui: &Ui, t: &Tokens, origin: Pos2, width: f32) {
    let h = t.row_height();
    for (i, (name, hex)) in LAYERS.into_iter().enumerate() {
        let row = Rect::from_min_size(origin + vec2(0.0, i as f32 * h), vec2(width, h));
        let swatch = Rect::from_center_size(
            pos2(row.left() + t.space.row_x + 7.0, row.center().y),
            Vec2::splat(14.0),
        );
        let color = Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        ui.painter().rect_filled(swatch, t.radius.mark, color);
        let at = row.with_min_x(swatch.right() + 8.0);
        let ink = t.color.text.primary;
        w::text_in(ui, at, Align2::LEFT_CENTER, &t.ty.body, name, ink);
    }
}

// --- Demos ----------------------------------------------------------------------------

fn controls(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    ui.horizontal(|ui| {
        w::button(ui, t, St::Live, Kind::Secondary, None, "Cancel");
        w::button(ui, t, St::Live, Kind::Primary, None, "Save");
        w::button(ui, t, St::Live, Kind::Flat, Some("find"), "Find");
        if w::icon_button(ui, t, St::Live, "grid", d.toggle, t.icon).clicked() {
            d.toggle = !d.toggle;
        }
    });
    ui.horizontal(|ui| {
        let check = if d.check { Check::On } else { Check::Off };
        if w::checkbox(ui, t, St::Live, check, "Check").clicked() {
            d.check = !d.check;
        }
        for (i, name) in ["One", "Two"].into_iter().enumerate() {
            if w::radio(ui, t, St::Live, d.radio == i, name).clicked() {
                d.radio = i;
            }
        }
        if w::switch(ui, t, St::Live, d.switch, "Switch").clicked() {
            d.switch = !d.switch;
        }
    });
    let field = Field {
        hint: "Click in: the focus ring is at once",
        ..Default::default()
    };
    w::text_field(ui, t, St::Live, &mut d.text, W, field);
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        let shown = anim::glide(&ctx, id("progress"), d.progress, p.progress);
        w::progress(ui, t, &t.color.accent, shown.clamp(0.0, 1.0), 240.0);
        if w::button(ui, t, St::Live, Kind::Secondary, None, "Step").clicked() {
            d.progress = if d.progress >= 1.0 { 0.0 } else { d.progress + 0.25 };
        }
    });
    note(ui, t, "Hover and press the buttons; flip the toggle, the check, the radios and the switch. Step moves the progress bar by a quarter.");
}

fn tabs(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let titles = [
        ("document_schematic", "Sheet 1"),
        ("document_pcb", "Main board"),
        ("rules", "Rules"),
    ];
    let (area, _) = ui.allocate_exact_size(vec2(W, t.tab_height() + 64.0), Sense::hover());
    let strip = area.with_max_y(area.top() + t.tab_height());
    let body = area.with_min_y(strip.bottom());
    w::card(ui, t, body);
    let mut rects = Vec::new();
    let row = strip.with_min_x(strip.left() + 6.0);
    within(ui, row, Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (i, (icon, title)) in titles.into_iter().enumerate() {
            let kind = if i == d.tab { TabKind::Front } else { TabKind::Back };
            let tab = Tab {
                kind,
                icon,
                title,
                modified: false,
                out_of_sync: false,
                window_active: true,
            };
            let r = w::tab(ui, t, St::Live, tab);
            if r.clicked() {
                d.tab = i;
            }
            rects.push(r.rect);
        }
    });
    // The active Document's accent line: fades in on the new tab, or glides to it.
    let line = |ui: &Ui, left: f32, right: f32, k: f32| {
        let bar = Rect::from_min_max(
            pos2(left + 10.0, strip.top()),
            pos2(right - 10.0, strip.top() + t.accent_line),
        );
        let under = egui::CornerRadius { nw: 0, ne: 0, sw: 2, se: 2 };
        let ink = t.color.accent.text.gamma_multiply(k.clamp(0.0, 1.0));
        ui.painter().rect_filled(bar, under, ink);
    };
    let ctx = ui.ctx().clone();
    if p.glide {
        let left = anim::glide(&ctx, id("tab-left"), rects[d.tab].left(), p.tab);
        let right = anim::glide(&ctx, id("tab-right"), rects[d.tab].right(), p.tab);
        line(ui, left, right, 1.0);
    } else {
        for (i, r) in rects.iter().enumerate() {
            let k = anim::value(&ctx, id("tab-line").with(i), i == d.tab, p.tab);
            if k > 0.0 {
                line(ui, r.left(), r.right(), k);
            }
        }
    }
    let text = format!("{}: what a tab holds swaps at once.", titles[d.tab].1);
    let ink = t.color.text.muted;
    w::text_in(ui, body, Align2::CENTER_CENTER, &t.ty.body, &text, ink);
}

#[derive(Clone, Copy)]
struct Tip {
    hot: Option<usize>,
    since: f64,
    /// Another tooltip was showing a moment ago: this one shows without the wait.
    warm: bool,
    last_seen: f64,
}

fn tooltips(ui: &mut Ui, t: &Tokens, p: &Profile) {
    let tools = [
        ("select", "Select", "Esc"),
        ("move", "Move", "M"),
        ("track", "Route Track", "X"),
        ("via", "Place Via", "V"),
        ("measure", "Measure", "Ctrl+M"),
    ];
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let mut tip = ctx.data(|m| m.get_temp::<Tip>(id("tip"))).unwrap_or(Tip {
        hot: None,
        since: now,
        warm: false,
        last_seen: -10.0,
    });
    let mut hot = None;
    let mut rects = [Rect::NOTHING; 5];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, (icon, ..)) in tools.into_iter().enumerate() {
            let r = w::icon_button(ui, t, St::Live, icon, false, t.icon_bar);
            if r.hovered() {
                hot = Some(i);
            }
            rects[i] = r.rect;
        }
    });
    if hot != tip.hot {
        tip.warm = hot.is_some() && now - tip.last_seen < 0.4;
        tip.hot = hot;
        tip.since = now;
    }
    let delay = f64::from(p.tooltip_delay);
    for (i, (_, name, keys)) in tools.into_iter().enumerate() {
        let due = tip.hot == Some(i) && (tip.warm || now - tip.since >= delay);
        if due {
            tip.last_seen = now;
        }
        let tr = if tip.warm { Tr::NONE } else { p.tooltip };
        let k = anim::value(&ctx, id("tip").with(i), due, tr);
        if k <= 0.0 {
            continue;
        }
        let name_w = w::width(ui, &t.ty.body, name);
        let keys_w = w::width(ui, &t.ty.small, keys) + 10.0;
        let size = vec2(name_w + 8.0 + keys_w + 20.0, t.ty.body.line + 12.0);
        let rect = Rect::from_min_size(
            pos2(rects[i].center().x - size.x / 2.0, rects[i].bottom() + 6.0),
            size,
        );
        overlay(ui, "tips", |ui| {
            present(ui, k, rect.center_top(), vec2(0.0, -p.travel), 1.0, |ui| {
                w::popup(ui, t, rect);
                let inner = rect.shrink2(vec2(10.0, 6.0));
                let ink = t.color.text.primary;
                w::text_in(ui, inner, Align2::LEFT_CENTER, &t.ty.body, name, ink);
                key_chip(ui, t, inner.right_center(), keys);
            });
        });
    }
    if tip.hot.is_some() && !tip.warm && now - tip.since < delay {
        let wait = delay - (now - tip.since);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    ctx.data_mut(|m| m.insert_temp(id("tip"), tip));
    ui.add_space(t.ty.body.line + 16.0);
    let text = format!(
        "Rest on a Tool for {} ms; then move along the row: the next tooltip shows without the wait and without a fade.",
        (p.tooltip_delay * 1000.0).round()
    );
    note(ui, t, &text);
}

/// A menu or a combo list under `anchor`. Returns the item picked.
#[allow(clippy::too_many_arguments)]
fn dropdown(
    ui: &mut Ui,
    t: &Tokens,
    p: &Profile,
    name: &str,
    open: &mut bool,
    anchor: Rect,
    width: f32,
    items: &[Item<'_>],
) -> Option<usize> {
    let k = anim::value(ui.ctx(), id(name), *open, p.menu);
    if k <= 0.0 {
        return None;
    }
    let height = items.len() as f32 * t.row_height() + 2.0 * t.space.menu;
    let rect = Rect::from_min_size(anchor.left_bottom() + vec2(0.0, 4.0), vec2(width, height));
    let mut picked = None;
    let live = *open;
    overlay(ui, name, |ui| {
        present(ui, k, rect.left_top(), vec2(0.0, -p.travel), p.menu_scale, |ui| {
            w::popup(ui, t, rect);
            within(ui, rect.shrink(t.space.menu), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (i, item) in items.iter().enumerate() {
                    let st = if live { St::Live } else { St::Rest };
                    let row_w = width - 2.0 * t.space.menu;
                    if w::menu_item(ui, t, st, row_w, *item).clicked() && live {
                        picked = Some(i);
                    }
                }
            });
        });
    });
    if picked.is_some() || pressed_outside(ui, &[rect, anchor]) {
        *open = false;
    }
    picked
}

fn menus(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let widths = ["Any Layer", "Top", "Bottom", "Inner 1"];
    let (mut menu_at, mut combo_at) = (Rect::NOTHING, Rect::NOTHING);
    ui.horizontal(|ui| {
        let kind = if d.menu { Kind::On } else { Kind::Flat };
        let r = w::button(ui, t, St::Live, kind, None, "Edit");
        if r.clicked() {
            d.menu = !d.menu;
            d.combo = false;
        }
        menu_at = r.rect;
        ui.add_space(W - r.rect.width() - 150.0 - 2.0 * t.space.gap);
        let r = w::combo(ui, t, St::Live, widths[d.combo_value], 150.0);
        if r.clicked() {
            d.combo = !d.combo;
            d.menu = false;
        }
        combo_at = r.rect;
    });
    let item = |icon, text, shortcut| Item {
        icon,
        text,
        shortcut,
        ..Default::default()
    };
    let edit = [
        item(Some("undo"), "Undo Move", "Ctrl+Z"),
        item(Some("redo"), "Redo", "Ctrl+Shift+Z"),
        item(Some("cut"), "Cut", "Ctrl+X"),
        item(Some("copy"), "Copy", "Ctrl+C"),
        item(Some("paste"), "Paste", "Ctrl+V"),
    ];
    dropdown(ui, t, p, "menu", &mut d.menu, menu_at, 230.0, &edit);
    let options: Vec<Item<'_>> = widths
        .iter()
        .enumerate()
        .map(|(i, text)| Item {
            text,
            checked: i == d.combo_value,
            ..Default::default()
        })
        .collect();
    if let Some(i) = dropdown(ui, t, p, "combo", &mut d.combo, combo_at, 150.0, &options) {
        d.combo_value = i;
    }
    ui.add_space(5.0 * t.row_height() + 2.0 * t.space.menu + 4.0);
    note(ui, t, "Open the menu and the combo list; close them by picking or by clicking away.");
}

fn list(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let nets = [
        ("GND", "38"),
        ("+3V3", "21"),
        ("+5V", "9"),
        ("USB_D+", "4"),
        ("USB_D−", "4"),
        ("SWDIO", "2"),
    ];
    let h = t.row_height();
    let ctx = ui.ctx().clone();
    let start = ui.cursor().min;
    let mark = |ui: &Ui, y: f32, k: f32| {
        let rect = Rect::from_min_size(pos2(start.x, y), vec2(W - 12.0, h));
        let fill = t.color.accent.subtle.gamma_multiply(k.clamp(0.0, 1.0));
        ui.painter().rect_filled(rect, t.radius.row, fill);
    };
    if p.glide {
        let y = anim::glide(&ctx, id("row-y"), start.y + d.row as f32 * h, p.select);
        mark(ui, y, 1.0);
    } else {
        for i in 0..nets.len() {
            let k = anim::value(&ctx, id("row").with(i), i == d.row, p.select);
            if k > 0.0 {
                mark(ui, start.y + i as f32 * h, k);
            }
        }
    }
    // The scrollbar widens while the pointer is over the list.
    let whole = Rect::from_min_size(start, vec2(W, nets.len() as f32 * h));
    let k = anim::value(&ctx, id("scrollbar"), ui.rect_contains_pointer(whole), p.hover);
    let width = egui::lerp(4.0..=8.0, k);
    let thumb = Rect::from_min_size(
        pos2(whole.right() - width - 1.0, whole.top() + 4.0),
        vec2(width, whole.height() * 0.45),
    );
    let ink = t.color.text.muted.gamma_multiply(egui::lerp(0.45..=0.7, k));
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (i, (name, pads)) in nets.into_iter().enumerate() {
            let (rect, r) = w::row(ui, t, St::Live, Sel::None, W - 12.0, h);
            let inner = rect.shrink2(vec2(t.space.row_x, 0.0));
            let ink = t.color.text.primary;
            w::text_in(ui, inner, Align2::LEFT_CENTER, &t.ty.body, name, ink);
            let ink = t.color.text.muted;
            w::text_in(ui, inner, Align2::RIGHT_CENTER, &t.ty.numeric, pads, ink);
            if r.clicked() {
                d.row = i;
            }
        }
    });
    ui.painter().rect_filled(thumb, width / 2.0, ink);
    note(ui, t, "Click rows. The scrollbar widens while the pointer is over the list.");
}

fn sections(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let groups: [(&str, &[(&str, &str)]); 2] = [
        (
            "Properties",
            &[("Net", "GND"), ("Width", "0.25 mm"), ("Layer", "Top")],
        ),
        ("Net Classes", &[("Default", "41 Nets"), ("Power", "6 Nets")]),
    ];
    let h = t.row_height();
    let ctx = ui.ctx().clone();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (g, (title, rows)) in groups.into_iter().enumerate() {
            let k = anim::value(&ctx, id("section").with(g), d.sections[g], p.section);
            let k = k.clamp(0.0, 1.0);
            let (rect, r) = w::row(ui, t, St::Live, Sel::None, W, h);
            if r.clicked() {
                d.sections[g] = !d.sections[g];
            }
            let c = &t.color;
            let at = pos2(rect.left() + t.space.row_x + 5.0, rect.center().y);
            chevron(ui, at, k, c.text.muted);
            let text = rect.with_min_x(rect.left() + t.space.row_x + 18.0);
            w::text_in(ui, text, Align2::LEFT_CENTER, &t.ty.title, title, c.text.primary);
            let count = rows.len().to_string();
            let inner = rect.shrink2(vec2(t.space.row_x, 0.0));
            w::text_in(ui, inner, Align2::RIGHT_CENTER, &t.ty.numeric_small, &count, c.text.muted);
            if k <= 0.0 {
                continue;
            }
            // The rows are revealed from the top: laid out at full height, clipped.
            let full = rows.len() as f32 * h;
            let (clip, _) = ui.allocate_exact_size(vec2(W, (full * k).round()), Sense::hover());
            faded(ui, k, clip, |ui| {
                for (i, (name, value)) in rows.iter().enumerate() {
                    let row = Rect::from_min_size(clip.min + vec2(0.0, i as f32 * h), vec2(W, h));
                    let name_at = row.with_min_x(row.left() + t.space.row_x + 18.0);
                    w::text_in(ui, name_at, Align2::LEFT_CENTER, &t.ty.body, name, c.text.muted);
                    let value_at = row.with_min_x(row.left() + 140.0);
                    w::text_in(ui, value_at, Align2::LEFT_CENTER, &t.ty.body, value, c.text.primary);
                }
            });
        }
    });
    note(ui, t, "Click a section header.");
}

fn dock(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let win = mini(ui, t, WIN_H);
    let inside = win.shrink(6.0);
    let ctx = ui.ctx().clone();
    let k = anim::value(&ctx, id("dock"), d.dock, p.dock).clamp(0.0, 1.0);
    let (strip_w, open_w) = (36.0, 150.0);
    let dock_w = egui::lerp(strip_w..=open_w, k);
    let dock_at = Rect::from_min_size(inside.min, vec2(dock_w, inside.height()));
    let canvas = inside.with_min_x(dock_at.right() + 6.0);
    w::card(ui, t, canvas);
    let ink = t.color.text.muted;
    w::text_in(ui, canvas, Align2::CENTER_CENTER, &t.ty.small, "Canvas", ink);
    let small_button = |ui: &mut Ui, name: &str, at: Pos2, glyph: Glyph| {
        let rect = Rect::from_center_size(at, Vec2::splat(22.0));
        let r = ui.interact(rect, id(name), Sense::click());
        let hover = anim::value(ui.ctx(), id(name).with("hover"), r.hovered(), p.hover);
        let fill = t.color.fill.hover.gamma_multiply(hover);
        ui.painter().rect_filled(rect, t.radius.row, fill);
        w::glyph(ui, glyph, at, 10.0, t.color.text.muted);
        r.clicked()
    };
    if k > 0.0 {
        // Open: a card with a header and rows, clipped as it narrows.
        let mut collapse = false;
        faded(ui, k, dock_at.expand(4.0), |ui| {
            w::card(ui, t, dock_at);
            let header = dock_at.with_max_y(dock_at.top() + t.tab_height());
            let title = header.with_min_x(header.left() + t.space.panel);
            let ink = t.color.text.primary;
            w::text_in(ui, title, Align2::LEFT_CENTER, &t.ty.title, "Layers", ink);
            if d.dock {
                let at = pos2(header.right() - 17.0, header.center().y);
                collapse = small_button(ui, "dock-collapse", at, Glyph::Minus);
            }
            faded(ui, 1.0, dock_at.shrink(1.0), |ui| {
                let origin = pos2(dock_at.left() + 4.0, header.bottom());
                layer_rows(ui, t, origin, open_w - 8.0);
            });
        });
        if collapse {
            d.dock = false;
        }
    }
    if !d.dock {
        // Collapsed: an icon strip on the base. The first icon opens its Panel as a flyout.
        let strip = Rect::from_min_size(inside.min, vec2(strip_w, inside.height()));
        let mut clicked = None;
        faded(ui, 1.0 - k, strip, |ui| {
            let at = pos2(strip.center().x, strip.top() + 15.0);
            if small_button(ui, "dock-expand", at, Glyph::ChevronRight) {
                clicked = Some(usize::MAX);
            }
            let column = Rect::from_min_size(
                pos2(strip.center().x - t.control_height() / 2.0, strip.top() + 32.0),
                vec2(t.control_height(), strip.height()),
            );
            within(ui, column, Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, icon) in ["layers", "inspect", "find"].into_iter().enumerate() {
                    let on = i == 0 && d.flyout;
                    if w::icon_button(ui, t, St::Live, icon, on, t.icon).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
        match clicked {
            Some(usize::MAX) => {
                d.dock = true;
                d.flyout = false;
            }
            Some(0) => d.flyout = !d.flyout,
            _ => {}
        }
        let k = anim::value(&ctx, id("flyout"), d.flyout, p.flyout);
        if k > 0.0 {
            let full = Rect::from_min_size(
                pos2(strip.right() + 2.0, inside.top()),
                vec2(open_w, inside.height()),
            );
            // Slides out from under the strip, or fades in beside it.
            let (shift, opacity) = if p.morph {
                (-(1.0 - k) * (open_w + 8.0), (k * 3.0).min(1.0))
            } else {
                (-(1.0 - k) * p.travel * 2.0, k)
            };
            let clip = win.with_min_x(strip.right());
            faded(ui, opacity, clip, |ui| {
                let rect = full.translate(vec2(shift, 0.0));
                w::popup(ui, t, rect);
                let header = rect.with_max_y(rect.top() + t.tab_height());
                let title = header.with_min_x(header.left() + t.space.panel);
                let ink = t.color.text.primary;
                w::text_in(ui, title, Align2::LEFT_CENTER, &t.ty.title, "Layers", ink);
                layer_rows(ui, t, pos2(rect.left() + 4.0, header.bottom()), open_w - 8.0);
            });
            if d.flyout && pressed_outside(ui, &[full, strip]) {
                d.flyout = false;
            }
        }
    }
    note(ui, t, "Collapse the Dock with −; in the strip, the Layers icon opens a flyout and › opens the Dock again.");
}

fn dialog(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let win = mini(ui, t, WIN_H);
    let inside = win.shrink(6.0);
    w::card(ui, t, inside);
    let c = &t.color;
    within(ui, inside.shrink(12.0), Layout::top_down(Align::Min), |ui| {
        w::label(ui, &t.ty.title, c.text.primary, "Net Classes");
        w::label(ui, &t.ty.body, c.text.muted, "Power · 6 Nets");
        ui.add_space(8.0);
        let st = if d.dialog { St::Rest } else { St::Live };
        let open = w::button(ui, t, st, Kind::Secondary, Some("delete"), "Delete Net Class…");
        if open.clicked() && !d.dialog {
            d.dialog = true;
        }
    });
    let k = anim::value(ui.ctx(), id("dialog"), d.dialog, p.dialog);
    if k > 0.0 {
        let scrim = c.surface.scrim.gamma_multiply(k.clamp(0.0, 1.0));
        ui.painter().rect_filled(win, t.radius.card, scrim);
        let pad = 16.0;
        let height = 2.0 * pad + t.ty.title.line + 4.0 + t.ty.body.line + 14.0 + t.control_height();
        let rect = Rect::from_center_size(win.center(), vec2(288.0, height));
        let mut close = false;
        present(ui, k, rect.center(), vec2(0.0, p.travel), p.dialog_scale, |ui| {
            w::window(ui, t, rect);
            within(ui, rect.shrink(pad), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing = vec2(t.space.gap, 0.0);
                w::label(ui, &t.ty.title, c.text.primary, "Delete Net Class “Power”?");
                ui.add_space(4.0);
                w::label(ui, &t.ty.body, c.text.muted, "Its 6 Nets go back to Default.");
                ui.add_space(14.0);
                let row = vec2(ui.available_width(), t.control_height());
                ui.allocate_ui_with_layout(row, Layout::right_to_left(Align::Center), |ui| {
                    close |= w::button(ui, t, St::Live, Kind::Danger, None, "Delete").clicked();
                    close |= w::button(ui, t, St::Live, Kind::Secondary, None, "Cancel").clicked();
                });
            });
        });
        if close {
            d.dialog = false;
        }
    }
    note(ui, t, "Open the dialog; either button closes it.");
}

fn palette(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let win = mini(ui, t, WIN_H);
    let c = &t.color;
    let bar = win.with_max_y(win.top() + 36.0);
    let field = Rect::from_center_size(bar.center(), vec2(190.0, 26.0));
    let r = ui.interact(field, id("command-field"), Sense::click());
    let hover = anim::value(ui.ctx(), id("command-field-hover"), r.hovered(), p.hover);
    ui.painter()
        .rect_filled(field, t.radius.control, c.fill.control);
    let fill = c.fill.hover.gamma_multiply(hover);
    ui.painter().rect_filled(field, t.radius.control, fill);
    w::glyph(ui, Glyph::Search, pos2(field.left() + 14.0, field.center().y), 12.0, c.text.muted);
    w::text_in(ui, field, Align2::CENTER_CENTER, &t.ty.small, "usb_c_mcu · Main board", c.text.muted);
    let body = win.shrink(6.0).with_min_y(bar.bottom());
    w::card(ui, t, body);
    w::text_in(ui, body, Align2::CENTER_BOTTOM, &t.ty.small, "Canvas", c.text.muted);
    if r.clicked() {
        d.palette = !d.palette;
    }
    let k = anim::value(ui.ctx(), id("palette"), d.palette, p.palette);
    if k > 0.0 {
        let commands = [
            ("Route Track", "X"),
            ("Track Width…", "W"),
            ("Select Connected Tracks", ""),
            ("Unroute Selection", ""),
        ];
        let head = t.control_height() + 8.0;
        let height = head + 1.0 + commands.len() as f32 * t.row_height() + 2.0 * t.space.menu;
        let full = Rect::from_min_size(
            pos2(win.center().x - 144.0, field.top() - 2.0),
            vec2(288.0, height),
        );
        let live = d.palette;
        let mut picked = false;
        let mut content = |ui: &mut Ui| {
            let top = full.with_max_y(full.top() + head);
            w::glyph(ui, Glyph::Search, pos2(top.left() + 16.0, top.center().y), 12.0, c.text.muted);
            let typed = top.with_min_x(top.left() + 30.0);
            let drawn = w::text_in(ui, typed, Align2::LEFT_CENTER, &t.ty.body, "track", c.text.primary);
            ui.painter().vline(
                drawn.right() + 1.0,
                drawn.y_range().shrink(2.0),
                Stroke::new(1.5, c.accent.text),
            );
            ui.painter().hline(
                full.x_range(),
                top.bottom(),
                Stroke::new(t.hairline, c.stroke.hairline),
            );
            let rows = full.with_min_y(top.bottom() + 1.0).shrink(t.space.menu);
            within(ui, rows, Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (i, (name, keys)) in commands.into_iter().enumerate() {
                    let sel = if i == 0 { Sel::Live } else { Sel::None };
                    let st = if live { St::Live } else { St::Rest };
                    let (rect, r) = w::row(ui, t, st, sel, rows.width(), t.row_height());
                    let inner = rect.shrink2(vec2(t.space.row_x, 0.0));
                    w::text_in(ui, inner, Align2::LEFT_CENTER, &t.ty.body, name, c.text.primary);
                    if !keys.is_empty() {
                        key_chip(ui, t, inner.right_center(), keys);
                    }
                    picked |= r.clicked() && live;
                }
            });
        };
        overlay(ui, "palette", |ui| {
            if p.morph {
                // Grows out of the command field: the window's rectangle travels from the
                // field's to its own, and what it holds is revealed inside it.
                let rect = field.lerp_towards(&full, k);
                faded(ui, (k * 4.0).min(1.0), rect.expand(60.0), |ui| w::window(ui, t, rect));
                faded(ui, k, rect, |ui| content(ui));
            } else {
                present(ui, k, full.center_top(), vec2(0.0, -p.travel), 1.0, |ui| {
                    w::window(ui, t, full);
                    content(ui);
                });
            }
        });
        if picked || (live && pressed_outside(ui, &[full, field])) {
            d.palette = false;
        }
    }
    note(ui, t, "Click the command field; pick a Command or click away.");
}

fn active_bar(ui: &mut Ui, t: &Tokens, p: &Profile, d: &mut Demo) {
    let (canvas, _) = ui.allocate_exact_size(vec2(W, 132.0), Sense::hover());
    let c = &t.color;
    ui.painter().rect_filled(canvas, t.radius.card, c.surface.well);
    ui.painter().rect_stroke(
        canvas,
        t.radius.card,
        Stroke::new(t.hairline, c.stroke.hairline),
        StrokeKind::Inside,
    );
    let mut y = canvas.top() + 14.0;
    while y < canvas.bottom() - 6.0 {
        let mut x = canvas.left() + 14.0;
        while x < canvas.right() - 6.0 {
            ui.painter().circle_filled(pos2(x, y), 0.8, c.stroke.strong);
            x += 20.0;
        }
        y += 20.0;
    }
    let tools = ["select", "move", "track", "via", "zone", "measure"];
    let (side, pad, gap) = (t.control_height(), 4.0, 2.0);
    let bar_w = |n: usize| n as f32 * side + (n - 1) as f32 * gap + 2.0 * pad;
    let bar = Rect::from_min_size(
        pos2(canvas.center().x - bar_w(tools.len()) / 2.0, canvas.top() + 12.0),
        vec2(bar_w(tools.len()), side + 2.0 * pad),
    );
    w::popup(ui, t, bar);
    let ctx = ui.ctx().clone();
    if p.glide {
        // The on state glides from the old Tool to the new one.
        let x = bar.left() + pad + d.tool as f32 * (side + gap);
        let x = anim::glide(&ctx, id("tool-x"), x, p.bar);
        let on = Rect::from_min_size(pos2(x, bar.top() + pad), Vec2::splat(side));
        ui.painter().rect_filled(on, t.radius.control, c.accent.subtle);
    }
    within(ui, bar.shrink(pad), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, icon) in tools.into_iter().enumerate() {
            let on = !p.glide && i == d.tool;
            if w::icon_button(ui, t, St::Live, icon, on, t.icon_bar).clicked() {
                d.tool = i;
            }
        }
    });
    // The Track Tool's own bar, at the bottom of the canvas while the Tool is active.
    let k = anim::value(&ctx, id("tool-bar"), d.tool == 2, p.bar);
    if k > 0.0 {
        let options = ["via", "layers", "lock", "units"];
        let rect = Rect::from_min_size(
            pos2(
                canvas.center().x - bar_w(options.len()) / 2.0,
                canvas.bottom() - 12.0 - side - 2.0 * pad,
            ),
            vec2(bar_w(options.len()), side + 2.0 * pad),
        );
        faded(ui, 1.0, canvas, |ui| {
            present(ui, k, rect.center_bottom(), vec2(0.0, p.travel), 1.0, |ui| {
                w::popup(ui, t, rect);
                within(ui, rect.shrink(pad), Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for icon in options {
                        w::icon_button(ui, t, St::Live, icon, false, t.icon_bar);
                    }
                });
            });
        });
    }
    note(ui, t, "Pick Tools. The third is the Track Tool: its bar shows at the bottom of the canvas while it is active.");
}

fn never(ui: &mut Ui, t: &Tokens) {
    let lines = [
        "The keyboard focus ring, and the caret.",
        "A press, and a selection made with the keyboard: they show at once.",
        "What a tab holds: Documents and Panels swap at once.",
        "Anything being dragged: tabs, splitters, sliders, scroll thumbs follow the pointer.",
        "Layout while the window is resized.",
        "The canvas: its zoom, flip and Layer-switch animation is its own Preference.",
    ];
    for line in lines {
        let wrap = ui.available_width();
        w::paragraph(ui, &t.ty.small, t.color.text.primary, &format!("·  {line}"), wrap);
    }
}

// --- The current profile, as a table --------------------------------------------------

fn timings(ui: &mut Ui, t: &Tokens, p: &Profile) {
    let rows: [(&str, Tr); 15] = [
        ("Hover fill, scrollbar", p.hover),
        ("Press fill", p.press),
        ("Check, radio, toggle on", p.check),
        ("Switch", p.switch),
        ("Tab accent line", p.tab),
        ("Menu, combo list", p.menu),
        ("Tooltip", p.tooltip),
        ("Flyout", p.flyout),
        ("Dock collapse", p.dock),
        ("Panel section", p.section),
        ("Dialog and scrim", p.dialog),
        ("Command Palette", p.palette),
        ("Selected row", p.select),
        ("Bars on the canvas", p.bar),
        ("Progress bar", p.progress),
    ];
    let c = &t.color;
    let width = ui.available_width();
    let cols = [width - 204.0, width - 140.0, width - 76.0];
    let h = t.ty.body.line + 4.0;
    let (head, _) = ui.allocate_exact_size(vec2(width, t.ty.small.line + 4.0), Sense::hover());
    for (x, name) in [(0.0, "Transition"), (cols[0], "In"), (cols[1], "Out"), (cols[2], "Easing")] {
        let at = head.with_min_x(head.left() + x);
        w::text_in(ui, at, Align2::LEFT_CENTER, &t.ty.small, name, c.text.muted);
    }
    w::hairline(ui, t, width);
    let ms = |s: f32| {
        if s <= 0.0 {
            "at once".to_owned()
        } else {
            format!("{} ms", (s * 1000.0).round())
        }
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (name, tr) in rows {
            let (row, _) = ui.allocate_exact_size(vec2(width, h), Sense::hover());
            let still = tr.enter <= 0.0 && tr.exit <= 0.0;
            let ink = if still { c.text.muted } else { c.text.primary };
            w::text_in(ui, row, Align2::LEFT_CENTER, &t.ty.body, name, ink);
            let cell = |x: f32| row.with_min_x(row.left() + x);
            w::text_in(ui, cell(cols[0]), Align2::LEFT_CENTER, &t.ty.numeric, &ms(tr.enter), ink);
            w::text_in(ui, cell(cols[1]), Align2::LEFT_CENTER, &t.ty.numeric, &ms(tr.exit), ink);
            let ease = if still { "" } else { tr.ease.name() };
            w::text_in(ui, cell(cols[2]), Align2::LEFT_CENTER, &t.ty.small, ease, c.text.muted);
        }
    });
    ui.add_space(6.0);
    w::hairline(ui, t, width);
    ui.add_space(6.0);
    let yes = |on: bool, yes: &str, no: &str| if on { yes.to_owned() } else { no.to_owned() };
    let facts = [
        format!("Tooltip wait: {} ms; none when another tooltip was just showing.", (p.tooltip_delay * 1000.0).round()),
        format!("Travel while fading in: {} px.", p.travel),
        format!("Starting scale: menus {}%, dialogs {}%.", (p.menu_scale * 100.0).round(), (p.dialog_scale * 100.0).round()),
        yes(
            p.glide,
            "The accent line, the selected row and a bar's on state glide from the old item to the new one.",
            "The accent line, the selected row and a bar's on state change in place.",
        ),
        yes(
            p.morph,
            "The Command Palette grows out of the command field; a flyout slides out from under the strip.",
            "The Command Palette and a flyout appear in place.",
        ),
        yes(
            anim::animate(),
            "Animate is on.",
            "Animate is off: everything happens at once. The tooltip wait stays.",
        ),
    ];
    for fact in facts {
        w::paragraph(ui, &t.ty.small, c.text.muted, &fact, width);
    }
}
