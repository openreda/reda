//! The engine and the tokens it plays, with the clock driven by hand.

use egui::{
    Color32, Context, FullOutput, Id, LayerId, Order, Pos2, RawInput, Rect, Shape, ViewportId,
    pos2, vec2,
};

use super::super::{Easing, Motion, Settings, Tokens, Transition, tokens};
use super::system::{self, PortalValue};
use super::{Look, arrive, eased, follow, layer, toggle};

const EPS: f32 = 1e-4;

/// One frame at `time` seconds, running `f` with the context.
fn frame(ctx: &Context, time: f64, f: impl FnMut(&Context)) -> FullOutput {
    let mut f = f;
    let input = RawInput {
        time: Some(time),
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| f(ui.ctx()));
    out.textures_delta.clear();
    out
}

/// A context that has drawn a first, empty frame (which always asks for another).
fn warm() -> Context {
    let ctx = Context::default();
    frame(&ctx, -1.0, |_| {});
    frame(&ctx, -0.5, |_| {});
    ctx
}

fn repaints(out: &FullOutput) -> bool {
    out.viewport_output
        .get(&ViewportId::ROOT)
        .is_some_and(|v| v.repaint_delay.is_zero())
}

fn crisp() -> Motion {
    tokens::motion(true)
}

fn still() -> Motion {
    tokens::motion(false)
}

/// Every transition the spec's table lists, by name.
fn transitions(m: &Motion) -> [(&'static str, Transition); 15] {
    [
        ("hover", m.hover),
        ("press", m.press),
        ("toggle", m.toggle),
        ("switch", m.switch),
        ("tab_line", m.tab_line),
        ("menu", m.menu),
        ("tooltip", m.tooltip),
        ("flyout", m.flyout),
        ("dock_collapse", m.dock_collapse),
        ("section", m.section),
        ("dialog", m.dialog),
        ("palette", m.palette),
        ("selected_row", m.selected_row),
        ("canvas_bar", m.canvas_bar),
        ("progress", m.progress),
    ]
}

#[test]
fn the_crisp_tokens_are_two_durations_one_easing_and_leave_faster() {
    let m = crisp();
    assert!(m.animate);
    assert_eq!((m.fast, m.base), (0.09, 0.14));
    for (name, tr) in transitions(&m) {
        for time in [Some(tr.enter), tr.leave].into_iter().flatten() {
            assert!(
                [0.0, m.fast, m.base].contains(&time),
                "{name}: {time} s is not one of the two durations"
            );
        }
        let linear = name == "hover";
        assert_eq!(tr.easing == Easing::Linear, linear, "{name}");
        if let Some(leave) = tr.leave
            && tr.enter > 0.0
        {
            assert!(leave <= tr.enter, "{name} leaves slower than it came");
        }
    }
    // Immediate: hover arriving, a press arriving, and selection.
    assert_eq!(m.hover.enter, 0.0);
    assert_eq!(m.press.enter, 0.0);
    assert_eq!(
        (m.selected_row.enter, m.selected_row.leave),
        (0.0, Some(0.0))
    );
    // 4 px of travel on arrival, 8 for a flyout; a dialog grows from 97 %.
    assert_eq!(
        (m.travel, m.flyout_travel, m.dialog_scale),
        (4.0, 8.0, 0.97)
    );
    assert_eq!((m.tooltip_wait, m.tooltip_chain), (0.5, 0.4));
}

#[test]
fn with_animate_off_every_transition_is_at_once_but_the_tooltip_still_waits() {
    let m = still();
    assert!(!m.animate);
    for (name, tr) in transitions(&m) {
        assert_eq!(tr.enter, 0.0, "{name}");
        assert!(tr.leave.unwrap_or(0.0) == 0.0, "{name}");
    }
    assert_eq!((m.travel, m.flyout_travel, m.dialog_scale), (0.0, 0.0, 1.0));
    assert_eq!((m.tooltip_wait, m.tooltip_chain), (0.5, 0.4));
    let off = Settings {
        animate: Some(false),
        ..Settings::default()
    };
    assert_eq!(Tokens::new(egui::Theme::Dark, &off).motion, m);
    assert_eq!(
        Tokens::new(egui::Theme::Dark, &Settings::default()).motion,
        crisp()
    );
    // egui's own transitions (popup fade-in, scrollbar) go with it.
    assert_eq!(
        super::super::style(&Tokens::new(egui::Theme::Dark, &off)).animation_time,
        0.0
    );
    let on = super::super::style(&Tokens::new(egui::Theme::Dark, &Settings::default()));
    assert_eq!(on.animation_time, 0.09);
    assert_eq!(on.interaction.tooltip_delay, 0.5);
}

#[test]
fn ease_out_is_cubic_and_linear_is_linear() {
    assert_eq!(eased(Easing::Out, 0.0), 0.0);
    assert!((eased(Easing::Out, 0.5) - 0.875).abs() < EPS);
    assert_eq!(eased(Easing::Out, 1.0), 1.0);
    assert!((eased(Easing::Linear, 0.25) - 0.25).abs() < EPS);
    assert_eq!(eased(Easing::Out, 2.0), 1.0);
}

#[test]
fn a_toggle_enters_and_leaves_in_its_own_times() {
    let ctx = warm();
    let id = Id::NULL.with("flyout");
    let tr = crisp().flyout; // 140 in, 90 out
    let at = |time: f64, on: bool| {
        let mut v = 0.0;
        let out = frame(&ctx, time, |ctx| v = toggle(ctx, id, on, tr));
        (v, repaints(&out))
    };
    // Seen first while off: off, at rest.
    assert_eq!(at(0.0, false), (0.0, false));
    // On at t = 1: it starts there and gets there 140 ms later.
    assert_eq!(at(1.0, true), (0.0, true));
    let (half, moving) = at(1.07, true);
    assert!((half - 0.875).abs() < EPS && moving, "{half}");
    assert_eq!(at(1.1401, true).0, 1.0);
    // Once there it asks for no more frames (egui gives one more after a request).
    at(1.5, true);
    assert_eq!(at(1.6, true), (1.0, false));
    // Off at t = 2: it is gone 90 ms later.
    assert_eq!(at(2.0, false), (1.0, true));
    let (half, _) = at(2.045, false);
    assert!((half - 0.125).abs() < EPS, "{half}");
    assert_eq!(at(2.0901, false).0, 0.0);
}

#[test]
fn a_hover_fill_arrives_at_once_and_fades_out_linearly_in_90_ms() {
    let ctx = warm();
    let id = Id::NULL.with("hover");
    let tr = crisp().hover;
    let at = |time: f64, on: bool| {
        let mut v = 0.0;
        frame(&ctx, time, |ctx| v = toggle(ctx, id, on, tr));
        v
    };
    assert_eq!(at(0.0, false), 0.0);
    assert_eq!(at(0.5, true), 1.0);
    assert_eq!(at(1.0, false), 1.0);
    assert!((at(1.045, false) - 0.5).abs() < EPS);
    assert!((at(1.0675, false) - 0.25).abs() < EPS);
    assert_eq!(at(1.09, false), 0.0);
}

#[test]
fn what_is_first_seen_on_is_on_without_a_transition() {
    let ctx = warm();
    let mut v = 0.0;
    let out = frame(&ctx, 0.0, |ctx| {
        v = toggle(ctx, Id::NULL.with("ticked"), true, crisp().toggle)
    });
    assert_eq!(v, 1.0);
    assert!(!repaints(&out));
}

#[test]
fn a_progress_bar_catches_up_with_its_value_in_140_ms() {
    let ctx = warm();
    let id = Id::NULL.with("progress");
    let tr = crisp().progress;
    let at = |time: f64, target: f32| {
        let mut v = 0.0;
        frame(&ctx, time, |ctx| v = follow(ctx, id, target, tr));
        v
    };
    assert_eq!(at(0.0, 0.2), 0.2);
    assert_eq!(at(1.0, 0.6), 0.2);
    assert!((at(1.07, 0.6) - (0.2 + 0.4 * 0.875)).abs() < EPS);
    assert_eq!(at(1.14, 0.6), 0.6);
}

#[test]
fn an_arrival_starts_again_after_a_pass_without_it() {
    let ctx = warm();
    let id = Id::NULL.with("menu");
    let tr = crisp().menu; // 90 in
    let at = |time: f64, shown: bool| {
        let mut v = None;
        frame(&ctx, time, |ctx| {
            if shown {
                v = Some(arrive(ctx, id, tr));
            }
        });
        v
    };
    assert_eq!(at(0.0, true), Some(0.0));
    assert!((at(0.045, true).unwrap() - 0.875).abs() < EPS);
    assert_eq!(at(0.09, true), Some(1.0));
    assert_eq!(at(0.5, true), Some(1.0));
    assert_eq!(at(0.6, false), None);
    assert_eq!(at(0.7, true), Some(0.0));
}

#[test]
fn with_animate_off_nothing_moves_and_nothing_repaints() {
    let ctx = warm();
    let m = still();
    let mut seen = Vec::new();
    for (i, on) in [false, true, false].into_iter().enumerate() {
        let out = frame(&ctx, i as f64, |ctx| {
            for (name, tr) in transitions(&m) {
                let v = toggle(ctx, Id::NULL.with(name), on, tr);
                seen.push((name, on, v));
            }
            let _ = arrive(ctx, Id::NULL.with(("arrive", i)), m.dialog);
        });
        assert!(!repaints(&out), "frame {i} asked for another");
    }
    for (name, on, v) in seen {
        assert_eq!(v, f32::from(u8::from(on)), "{name}");
    }
}

/// The colour that marks the test layer's rect.
const MARK: Color32 = Color32::from_rgb(200, 40, 40);

/// A frame at `time` that draws a 100 × 50 rect at (100, 100) on `layer` when `shown`,
/// as a dialog does; returns that rect as painted (place and alpha), if painted.
fn layer_frame(
    ctx: &Context,
    time: f64,
    shown: bool,
    look: Look,
    tr: Transition,
) -> Option<(Rect, u8)> {
    let id = LayerId::new(Order::Foreground, Id::NULL.with("test-layer"));
    let out = frame(ctx, time, |ctx| {
        if shown {
            let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 50.0));
            ctx.layer_painter(id).rect_filled(rect, 0.0, MARK);
            layer(ctx, id, look, tr);
        }
    });
    out.shapes.iter().find_map(|clipped| match &clipped.shape {
        Shape::Rect(r) if r.rect.size() == vec2(100.0, 50.0) => {
            let t = clipped.transform_after_tessellation;
            Some((t.mul_rect(r.rect), r.fill.a()))
        }
        _ => None,
    })
}

#[test]
fn a_layer_fades_and_travels_in_then_fades_out_after_its_owner_stops_drawing_it() {
    let ctx = warm();
    ctx.add_plugin(super::Plugin);
    let m = crisp();
    let look = Look::travel(vec2(0.0, -m.travel));
    let tr = m.palette; // 140 in, 90 out
    let home = Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 50.0));

    // The pass it appears: 4 px up and transparent.
    let (rect, alpha) = layer_frame(&ctx, 1.0, true, look, tr).unwrap();
    assert_eq!(rect, home.translate(vec2(0.0, -4.0)));
    assert_eq!(alpha, 0);
    // Half way through its 140 ms: 87.5 % of the way down, and as opaque.
    let (rect, alpha) = layer_frame(&ctx, 1.07, true, look, tr).unwrap();
    assert!((rect.top() - (100.0 - 0.5)).abs() < 0.01, "{rect:?}");
    assert!((i32::from(alpha) - 223).abs() <= 1, "{alpha}");
    // There.
    assert_eq!(layer_frame(&ctx, 1.14, true, look, tr), Some((home, 255)));
    assert_eq!(layer_frame(&ctx, 2.0, true, look, tr), Some((home, 255)));
    // Its owner stops drawing it: its last look is painted fading out for 90 ms.
    assert_eq!(layer_frame(&ctx, 3.0, false, look, tr), Some((home, 255)));
    let (_, alpha) = layer_frame(&ctx, 3.045, false, look, tr).unwrap();
    assert!((i32::from(alpha) - 32).abs() <= 1, "{alpha}");
    assert_eq!(layer_frame(&ctx, 3.09, false, look, tr), None);
    assert_eq!(layer_frame(&ctx, 4.0, false, look, tr), None);
}

#[test]
fn a_dialog_grows_from_97_percent_around_its_centre() {
    let ctx = warm();
    ctx.add_plugin(super::Plugin);
    let m = crisp();
    let home = Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 50.0));
    let look = Look::grow(m.dialog_scale, home.center());
    let (rect, _) = layer_frame(&ctx, 0.0, true, look, m.dialog).unwrap();
    assert!((rect.width() - 97.0).abs() < 0.01, "{rect:?}");
    assert!((rect.center() - home.center()).length() < 0.01);
    assert_eq!(
        layer_frame(&ctx, 0.14, true, look, m.dialog),
        Some((home, 255))
    );
}

#[test]
fn a_layer_with_animate_off_is_there_at_once_and_gone_at_once() {
    let ctx = warm();
    ctx.add_plugin(super::Plugin);
    let m = still();
    let look = Look::travel(vec2(0.0, -m.travel));
    let home = Rect::from_min_size(pos2(100.0, 100.0), vec2(100.0, 50.0));
    assert_eq!(
        layer_frame(&ctx, 0.0, true, look, m.dialog),
        Some((home, 255))
    );
    assert_eq!(layer_frame(&ctx, 1.0, false, look, m.dialog), None);
}

#[test]
fn a_menu_only_travels_because_egui_fades_it() {
    let ctx = warm();
    ctx.add_plugin(super::Plugin);
    let m = crisp();
    let look = Look::travel(vec2(-m.travel, 0.0)).without_fade();
    let (rect, alpha) = layer_frame(&ctx, 0.0, true, look, m.menu).unwrap();
    assert_eq!(rect.left(), 96.0);
    assert_eq!(alpha, 255);
    // It leaves at once.
    layer_frame(&ctx, 1.0, true, look, m.menu);
    assert_eq!(layer_frame(&ctx, 2.0, false, look, m.menu), None);
}

#[test]
fn a_failed_os_read_allows_motion() {
    assert!(!system::allows_motion(|| Ok(true)));
    assert!(system::allows_motion(|| Ok(false)));
    assert!(system::allows_motion(|| Err("no session bus".to_owned())));
}

#[test]
fn the_portal_is_read_standard_key_first_then_gnomes() {
    use PortalValue::{Flag, Number};
    let portal = |standard: Option<PortalValue>, gnome: Option<PortalValue>| {
        system::portal_reduces(|namespace, key| match (namespace, key) {
            ("org.freedesktop.appearance", "reduced-motion") => standard,
            ("org.gnome.desktop.interface", "enable-animations") => gnome,
            _ => None,
        })
    };
    // reduced-motion = 1 decides, whatever GNOME's key says.
    assert_eq!(portal(Some(Number(1)), Some(Flag(true))), Some(true));
    // 0, or missing: enable-animations decides.
    assert_eq!(portal(Some(Number(0)), Some(Flag(false))), Some(true));
    assert_eq!(portal(None, Some(Flag(false))), Some(true));
    assert_eq!(portal(None, Some(Flag(true))), Some(false));
    // 0 and no GNOME key: no preference. An unknown value counts as 0.
    assert_eq!(portal(Some(Number(0)), None), Some(false));
    assert_eq!(portal(Some(Number(7)), None), Some(false));
    // Neither key: no answer, which allows motion.
    assert_eq!(portal(None, None), None);
}

#[test]
fn an_unset_animate_follows_what_the_os_read_gave_at_launch() {
    let ctx = Context::default();
    assert!(system::remembered(&ctx), "nothing read allows motion");
    system::remember(&ctx, false);
    super::super::set(&ctx, &Settings::default());
    assert!(!super::super::TokensExt::tokens(&ctx).motion.animate);
    // A stored value wins over the OS.
    let on = Settings {
        animate: Some(true),
        ..Settings::default()
    };
    super::super::set(&ctx, &on);
    assert!(super::super::TokensExt::tokens(&ctx).motion.animate);
}
