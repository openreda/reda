//! Reading the wheel through the one mapping (docs/spec/v0.3/input-and-defaults.md#wheel-mapping):
//! each scroll goes to the action its modifiers are bound to, touchpad scrolls pan under
//! Auto and Always pan, and a pinch zooms. The canvas and the 3D view share it.

use egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, TouchPhase, Vec2, vec2};
use reda_ui::engine::preferences::{Touchpad, Wheel, WheelBinding, WheelPreset};
use reda_ui::wheel::{self, WheelGesture, WheelState};

/// The points one wheel line scrolls in these tests.
const LINE: f32 = 40.0;

fn notch(lines: f32, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, lines),
        phase: TouchPhase::Move,
        source: MouseWheelSource::Wheel,
        modifiers,
    }
}

fn finger(delta: Vec2, source: MouseWheelSource, modifiers: Modifiers) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        phase: TouchPhase::Move,
        source,
        modifiers,
    }
}

fn read(events: &[Event], prefs: &Wheel) -> WheelGesture {
    wheel::gesture(events, None, prefs, LINE, 600.0, &mut WheelState::default())
}

#[test]
fn the_reda_table_zooms_on_the_plain_wheel_and_scrolls_with_shift_and_ctrl() {
    let reda = Wheel::default();
    let up = read(&[notch(1.0, Modifiers::NONE)], &reda);
    assert_eq!(up.zoom, 40.0);
    assert_eq!(up.pan, Vec2::ZERO);
    let down = read(&[notch(-1.0, Modifiers::NONE)], &reda);
    assert_eq!(down.zoom, -40.0);
    // Shift: sideways; Ctrl: up and down. Wheel up moves the content right or down.
    let sideways = read(&[notch(1.0, Modifiers::SHIFT)], &reda);
    assert_eq!((sideways.zoom, sideways.pan), (0.0, vec2(40.0, 0.0)));
    let vertical = read(&[notch(-1.0, Modifiers::CTRL)], &reda);
    assert_eq!((vertical.zoom, vertical.pan), (0.0, vec2(0.0, -40.0)));
    // A combination nothing is bound to does nothing.
    let alt = read(&[notch(1.0, Modifiers::ALT)], &reda);
    assert_eq!(alt, WheelGesture::default());
}

#[test]
fn the_wheel_scrolls_table_scrolls_on_the_plain_wheel_and_zooms_with_ctrl() {
    let table = WheelPreset::WheelScrolls.table();
    let plain = read(&[notch(1.0, Modifiers::NONE)], &table);
    assert_eq!((plain.zoom, plain.pan), (0.0, vec2(0.0, 40.0)));
    let ctrl = read(&[notch(1.0, Modifiers::CTRL)], &table);
    assert_eq!((ctrl.zoom, ctrl.pan), (40.0, Vec2::ZERO));
    // Cmd counts as Ctrl.
    let cmd = read(&[notch(1.0, Modifiers::MAC_CMD)], &table);
    assert_eq!(cmd.zoom, 40.0);
}

#[test]
fn zoom_speed_scales_and_reverse_flips_the_wheel_zoom() {
    let mut prefs = Wheel {
        zoom_speed: 2.0,
        ..Wheel::default()
    };
    assert_eq!(read(&[notch(1.0, Modifiers::NONE)], &prefs).zoom, 80.0);
    prefs.reverse_zoom = true;
    assert_eq!(read(&[notch(1.0, Modifiers::NONE)], &prefs).zoom, -80.0);
}

#[test]
fn step_layer_steps_once_per_notch_and_keeps_part_steps() {
    let prefs = Wheel {
        step_layer: WheelBinding::parse("alt").unwrap(),
        ..Wheel::default()
    };
    // Wheel down is the next Layer.
    let down = read(
        &[notch(-1.0, Modifiers::ALT), notch(-1.0, Modifiers::ALT)],
        &prefs,
    );
    assert_eq!(
        (down.layer_steps, down.zoom, down.pan),
        (2, 0.0, Vec2::ZERO)
    );
    assert_eq!(read(&[notch(1.0, Modifiers::ALT)], &prefs).layer_steps, -1);
    // Smooth scrolling adds up to whole notches across frames.
    let mut state = WheelState::default();
    let quarter = [finger(
        vec2(0.0, -10.0),
        MouseWheelSource::Wheel,
        Modifiers::ALT,
    )];
    let mut steps = Vec::new();
    for _ in 0..4 {
        steps.push(wheel::gesture(&quarter, None, &prefs, LINE, 600.0, &mut state).layer_steps);
    }
    assert_eq!(steps, [0, 0, 0, 1]);
}

#[test]
fn touchpad_auto_pans_for_touchpad_scrolls_and_follows_the_table_for_wheels() {
    let auto = Wheel::default();
    assert_eq!(auto.touchpad, Touchpad::Auto);
    // Two fingers pan both ways at once, whatever is held.
    for source in [MouseWheelSource::Trackpad, MouseWheelSource::Momentum] {
        for held in [Modifiers::NONE, Modifiers::SHIFT] {
            let g = read(&[finger(vec2(5.0, -3.0), source, held)], &auto);
            assert_eq!(
                (g.zoom, g.pan),
                (0.0, vec2(5.0, -3.0)),
                "{source:?} {held:?}"
            );
        }
    }
    // A scroll of unknown source follows the table, precise (points) or in lines: only a
    // scroll the system reports as a touchpad's pans.
    let precise = read(
        &[finger(
            vec2(0.0, 6.0),
            MouseWheelSource::Unknown,
            Modifiers::NONE,
        )],
        &auto,
    );
    assert_eq!((precise.zoom, precise.pan), (6.0, Vec2::ZERO));
    let lines = Event::MouseWheel {
        unit: MouseWheelUnit::Line,
        delta: vec2(0.0, 1.0),
        phase: TouchPhase::Move,
        source: MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    };
    assert_eq!(read(&[lines], &auto).zoom, 40.0);
    assert_eq!(read(&[notch(1.0, Modifiers::NONE)], &auto).zoom, 40.0);
}

#[test]
fn a_pinch_zooms_in_every_touchpad_mode() {
    for touchpad in [Touchpad::Auto, Touchpad::Pan, Touchpad::Wheel] {
        let prefs = Wheel {
            touchpad,
            ..Wheel::default()
        };
        let g = read(&[Event::Zoom(1.5), Event::Zoom(2.0)], &prefs);
        assert_eq!((g.pinch, g.zoom, g.pan), (3.0, 0.0, Vec2::ZERO));
        // A touch screen's two-finger pinch, as egui measures it.
        let touch = wheel::gesture(
            &[],
            Some(1.25),
            &prefs,
            LINE,
            600.0,
            &mut WheelState::default(),
        );
        assert_eq!(touch.pinch, 1.25);
    }
    assert_eq!(read(&[], &Wheel::default()).pinch, 1.0);
}

#[test]
fn always_pan_takes_every_scroll_as_a_touchpads_and_same_as_wheel_none() {
    let pan = Wheel {
        touchpad: Touchpad::Pan,
        ..Wheel::default()
    };
    let g = read(&[notch(1.0, Modifiers::NONE)], &pan);
    assert_eq!((g.zoom, g.pan), (0.0, vec2(0.0, 40.0)));
    let wheel = Wheel {
        touchpad: Touchpad::Wheel,
        ..Wheel::default()
    };
    let g = read(
        &[finger(
            vec2(0.0, 6.0),
            MouseWheelSource::Trackpad,
            Modifiers::NONE,
        )],
        &wheel,
    );
    assert_eq!((g.zoom, g.pan), (6.0, Vec2::ZERO));
}
