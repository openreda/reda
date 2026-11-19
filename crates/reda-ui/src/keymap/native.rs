//! Preserve keys that egui-winit otherwise collapses: both digit rows become
//! Num0..Num9, and '*' is delivered as text without its modifiers.

use eframe::{AppCreator, EframeWinitApplication, NativeOptions, UserEvent};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};
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

impl ApplicationHandler<UserEvent> for Window {
    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, mut event: WindowEvent) {
        if let WindowEvent::KeyboardInput { event: key, .. } = &mut event
            && let Key::Character(text) = &key.logical_key
        {
            if text == "*" {
                // egui needs a key event to preserve the modifiers at the
                // instant of the press. Keep its companion text for editing.
                let direct = key.location == KeyLocation::Numpad
                    || key.key_without_modifiers().to_text() == Some("*");
                key.text.get_or_insert_with(|| text.clone());
                key.logical_key = Key::Named(if direct { NamedKey::F23 } else { NamedKey::F22 });
                key.physical_key = PhysicalKey::Code(KeyCode::F24);
            } else if key.location == KeyLocation::Numpad
                && text.len() == 1
                && text.as_bytes()[0].is_ascii_digit()
            {
                key.physical_key = PhysicalKey::Code(KeyCode::F24);
            }
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
