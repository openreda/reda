//! Preserve keys that egui-winit otherwise collapses: both digit rows become
//! Num0..Num9, and '*' is delivered as text without its modifiers.
//!
//! What egui then sees, as the Keymap reads it ([`Chord::from_event`](super::Chord)):
//! - a numpad digit has the physical key F24 (its logical key and text stay the digit);
//!   with Num Lock off it has no physical key, so it is only the key it acts as (End);
//! - a '*' is F23 (a dedicated multiply key) or F22 (a key typing it with Shift), with the
//!   physical key F24, or the digit when the key is on the top row (Shift+8 on US);
//! - a top-row digit key keeps its physical digit, whatever it types.

use eframe::{AppCreator, EframeWinitApplication, NativeOptions, UserEvent};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, KeyLocation, NamedKey, NativeKeyCode, PhysicalKey, SmolStr};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement as _;
use winit::window::WindowId;

pub(super) struct Window(EframeWinitApplication<'static>);

pub(crate) fn run(
    name: &str,
    options: NativeOptions,
    create: AppCreator<'static>,
) -> eframe::Result {
    let events = EventLoop::<UserEvent>::with_user_event().build()?;
    let app = eframe::create_native(name, options, create, &events);
    events.run_app(&mut Window(app))?;
    Ok(())
}

/// One key event's parts the adapter rewrites.
#[derive(Clone, Debug, PartialEq)]
struct KeyParts {
    logical: Key,
    physical: PhysicalKey,
    text: Option<SmolStr>,
}

/// Rewrites a key event at `location` whose key types `unmodified` without modifiers.
fn adapt(parts: &mut KeyParts, location: KeyLocation, unmodified: Option<&str>) {
    let numpad_digit = matches!(
        parts.physical,
        PhysicalKey::Code(
            KeyCode::Numpad0
                | KeyCode::Numpad1
                | KeyCode::Numpad2
                | KeyCode::Numpad3
                | KeyCode::Numpad4
                | KeyCode::Numpad5
                | KeyCode::Numpad6
                | KeyCode::Numpad7
                | KeyCode::Numpad8
                | KeyCode::Numpad9
        )
    );
    let top_row = matches!(
        parts.physical,
        PhysicalKey::Code(
            KeyCode::Digit0
                | KeyCode::Digit1
                | KeyCode::Digit2
                | KeyCode::Digit3
                | KeyCode::Digit4
                | KeyCode::Digit5
                | KeyCode::Digit6
                | KeyCode::Digit7
                | KeyCode::Digit8
                | KeyCode::Digit9
        )
    );
    match &parts.logical {
        Key::Character(text) if text == "*" => {
            // egui needs a key event to preserve the modifiers at the
            // instant of the press. Keep its companion text for editing.
            let direct = location == KeyLocation::Numpad || unmodified == Some("*");
            let text = text.clone();
            parts.text.get_or_insert(text);
            parts.logical = Key::Named(if direct { NamedKey::F23 } else { NamedKey::F22 });
            if !top_row {
                parts.physical = PhysicalKey::Code(KeyCode::F24);
            }
        }
        Key::Character(text)
            if location == KeyLocation::Numpad
                && text.len() == 1
                && text.as_bytes()[0].is_ascii_digit() =>
        {
            parts.physical = PhysicalKey::Code(KeyCode::F24);
        }
        // A numpad digit key with Num Lock off (End, the arrows) is not a digit.
        _ if numpad_digit => {
            parts.physical = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        }
        _ => {}
    }
}

impl ApplicationHandler<UserEvent> for Window {
    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, mut event: WindowEvent) {
        if let WindowEvent::KeyboardInput { event: key, .. } = &mut event {
            let unmodified = key.key_without_modifiers();
            let mut parts = KeyParts {
                logical: key.logical_key.clone(),
                physical: key.physical_key,
                text: key.text.clone(),
            };
            adapt(&mut parts, key.location, unmodified.to_text());
            key.logical_key = parts.logical;
            key.physical_key = parts.physical;
            key.text = parts.text;
        }
        self.0.window_event(events, id, event);
    }

    fn new_events(&mut self, events: &ActiveEventLoop, cause: StartCause) {
        self.0.new_events(events, cause);
    }
    fn resumed(&mut self, events: &ActiveEventLoop) {
        self.0.resumed(events);
    }
    fn user_event(&mut self, events: &ActiveEventLoop, event: UserEvent) {
        self.0.user_event(events, event);
    }
    fn device_event(&mut self, events: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.0.device_event(events, id, event);
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        self.0.about_to_wait(events);
    }
    fn suspended(&mut self, events: &ActiveEventLoop) {
        self.0.suspended(events);
    }
    fn exiting(&mut self, events: &ActiveEventLoop) {
        self.0.exiting(events);
    }
    fn memory_warning(&mut self, events: &ActiveEventLoop) {
        self.0.memory_warning(events);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapted(
        logical: Key,
        physical: KeyCode,
        text: Option<&str>,
        location: KeyLocation,
        unmodified: Option<&str>,
    ) -> KeyParts {
        let mut parts = KeyParts {
            logical,
            physical: PhysicalKey::Code(physical),
            text: text.map(SmolStr::new),
        };
        adapt(&mut parts, location, unmodified);
        parts
    }

    fn char(c: &str) -> Key {
        Key::Character(SmolStr::new(c))
    }

    #[test]
    fn a_top_row_digit_key_keeps_its_place_whatever_it_types() {
        // AZERTY's unshifted top-row 1 types `&`.
        let parts = adapted(
            char("&"),
            KeyCode::Digit1,
            Some("&"),
            KeyLocation::Standard,
            Some("&"),
        );
        assert_eq!(parts.physical, PhysicalKey::Code(KeyCode::Digit1));
        assert_eq!(parts.logical, char("&"));
    }

    #[test]
    fn a_numpad_digit_is_marked_and_with_num_lock_off_it_is_not_a_digit() {
        let on = adapted(
            char("3"),
            KeyCode::Numpad3,
            Some("3"),
            KeyLocation::Numpad,
            Some("3"),
        );
        assert_eq!(on.physical, PhysicalKey::Code(KeyCode::F24));
        assert_eq!(on.logical, char("3"));
        let off = adapted(
            Key::Named(NamedKey::PageDown),
            KeyCode::Numpad3,
            None,
            KeyLocation::Numpad,
            None,
        );
        assert_eq!(
            off.physical,
            PhysicalKey::Unidentified(NativeKeyCode::Unidentified)
        );
        assert_eq!(off.logical, Key::Named(NamedKey::PageDown));
    }

    #[test]
    fn an_asterisk_keeps_the_top_row_digit_it_was_typed_on() {
        // US Shift+8.
        let shifted = adapted(
            char("*"),
            KeyCode::Digit8,
            Some("*"),
            KeyLocation::Standard,
            Some("8"),
        );
        assert_eq!(shifted.logical, Key::Named(NamedKey::F22));
        assert_eq!(shifted.physical, PhysicalKey::Code(KeyCode::Digit8));
        assert_eq!(shifted.text.as_deref(), Some("*"));
        // The numpad multiply key.
        let multiply = adapted(
            char("*"),
            KeyCode::NumpadMultiply,
            None,
            KeyLocation::Numpad,
            Some("*"),
        );
        assert_eq!(multiply.logical, Key::Named(NamedKey::F23));
        assert_eq!(multiply.physical, PhysicalKey::Code(KeyCode::F24));
        assert_eq!(multiply.text.as_deref(), Some("*"));
        // Its own key elsewhere (German `*` is Shift on the key right of Ü).
        let german = adapted(
            char("*"),
            KeyCode::BracketRight,
            Some("*"),
            KeyLocation::Standard,
            Some("+"),
        );
        assert_eq!(german.logical, Key::Named(NamedKey::F22));
        assert_eq!(german.physical, PhysicalKey::Code(KeyCode::F24));
    }
}
