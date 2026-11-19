//! egui-winit normalizes both digit rows to Num0..Num9. Preserve the native
//! location before that translation, while leaving logical keys and typed text
//! available to text fields. All windows use the same adapter.

use eframe::{AppCreator, EframeWinitApplication, NativeOptions, UserEvent};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, KeyLocation, PhysicalKey};
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
            && key.location == KeyLocation::Numpad
            && let Key::Character(text) = &key.logical_key
            && text.len() == 1
            && text.as_bytes()[0].is_ascii_digit()
        {
            // A physical marker paired only with a logical digit. A normal F24
            // press still has F24 as its logical key and is handled normally.
            key.physical_key = PhysicalKey::Code(KeyCode::F24);
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
