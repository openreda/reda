//! The OS reduced-motion setting, read once at each launch
//! (docs/spec/v0.1/app.md#preferences, docs/research/reduced-motion.md on the
//! `research/reduced-motion` branch).
//!
//! While `general.animate` is unset, the chrome animates unless the OS asks for less
//! motion. Each platform has one small read, with no `unsafe`:
//!
//! - macOS: AppKit's `NSWorkspace.accessibilityDisplayShouldReduceMotion`.
//! - Windows: WinRT's `UISettings.AnimationsEnabled`.
//! - Linux: the settings portal over D-Bus, `org.freedesktop.appearance`
//!   `reduced-motion`, then GNOME's `org.gnome.desktop.interface` `enable-animations`.
//!
//! A read that fails, or a platform with no answer, counts as motion allowed. REDA does
//! not follow the setting while it runs.

use std::time::Duration;

/// What the OS said, read at launch: `Ok(true)` when it asks for reduced motion.
pub type Read = Result<bool, String>;

/// Whether the chrome may animate, given what the OS read gave: a failed read allows it.
pub fn allows_motion(read: impl FnOnce() -> Read) -> bool {
    match read() {
        Ok(reduce) => !reduce,
        Err(_) => true,
    }
}

/// A value the settings portal gave for one key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalValue {
    /// `reduced-motion`: 0 no preference, 1 reduced motion.
    Number(u32),
    /// `enable-animations`.
    Flag(bool),
}

/// The portal's namespace and key of the standard setting, and of GNOME's.
pub const REDUCED_MOTION: (&str, &str) = ("org.freedesktop.appearance", "reduced-motion");
pub const ENABLE_ANIMATIONS: (&str, &str) = ("org.gnome.desktop.interface", "enable-animations");

/// Whether the settings portal asks for reduced motion, reading one key at a time with
/// `read_one` (`None` when the portal has no such key or the call failed).
/// `reduced-motion` comes first and 1 means reduce; when it is missing or 0, GNOME's
/// `enable-animations` decides, and false means reduce. `None` when neither answers.
pub fn portal_reduces(read_one: impl Fn(&str, &str) -> Option<PortalValue>) -> Option<bool> {
    let standard = read_one(REDUCED_MOTION.0, REDUCED_MOTION.1);
    if standard == Some(PortalValue::Number(1)) {
        return Some(true);
    }
    match read_one(ENABLE_ANIMATIONS.0, ENABLE_ANIMATIONS.1) {
        Some(PortalValue::Flag(enabled)) => Some(!enabled),
        _ => standard.map(|_| false),
    }
}

/// The read of this platform.
pub fn read() -> Read {
    platform::read()
}

/// The read started at launch. On Linux it runs on its own thread, because a D-Bus call
/// to a portal that is not running waits on its activation; elsewhere it is one call,
/// made on the thread that asks.
pub struct AtLaunch(Inner);

enum Inner {
    Thread(std::sync::mpsc::Receiver<Read>),
    Here,
}

impl AtLaunch {
    /// Starts the read.
    pub fn start() -> AtLaunch {
        if !cfg!(target_os = "linux") {
            return AtLaunch(Inner::Here);
        }
        let (send, receive) = std::sync::mpsc::channel();
        let started = std::thread::Builder::new()
            .name("reda-reduced-motion".to_owned())
            .spawn(move || {
                let _ = send.send(read());
            });
        match started {
            Ok(_) => AtLaunch(Inner::Thread(receive)),
            Err(_) => AtLaunch(Inner::Here),
        }
    }

    /// Whether the chrome may animate, waiting at most `wait` for a read in flight. A
    /// read that has not answered by then counts as motion allowed.
    pub fn allows_motion(self, wait: Duration) -> bool {
        match self.0 {
            Inner::Thread(receive) => allows_motion(|| {
                receive
                    .recv_timeout(wait)
                    .unwrap_or_else(|e| Err(e.to_string()))
            }),
            Inner::Here => allows_motion(read),
        }
    }
}

/// How long the window waits at launch for the Linux read.
pub const LAUNCH_WAIT: Duration = Duration::from_millis(300);

#[cfg(target_os = "linux")]
mod platform {
    use zbus::zvariant::{OwnedValue, Value};

    use super::{PortalValue, Read};

    pub fn read() -> Read {
        let bus = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
        let portal = zbus::blocking::Proxy::new(
            &bus,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )
        .map_err(|e| e.to_string())?;
        let read_one = |namespace: &str, key: &str| {
            let value: OwnedValue = portal.call("ReadOne", &(namespace, key)).ok()?;
            portal_value(&value)
        };
        super::portal_reduces(read_one).ok_or_else(|| "the portal has neither key".to_owned())
    }

    /// The number or flag in a portal reply, unwrapping the extra variant the older
    /// `Read` method put around it.
    fn portal_value(value: &Value<'_>) -> Option<PortalValue> {
        match value {
            Value::U32(n) => Some(PortalValue::Number(*n)),
            Value::Bool(b) => Some(PortalValue::Flag(*b)),
            Value::Value(inner) => portal_value(inner),
            _ => None,
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::Read;

    pub fn read() -> Read {
        let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
        Ok(workspace.accessibilityDisplayShouldReduceMotion())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::Read;

    pub fn read() -> Read {
        let settings = windows::UI::ViewManagement::UISettings::new().map_err(|e| e.to_string())?;
        let enabled = settings.AnimationsEnabled().map_err(|e| e.to_string())?;
        Ok(!enabled)
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::Read;

    pub fn read() -> Read {
        Err("this platform has no reduced-motion setting REDA reads".to_owned())
    }
}

fn remembered_id() -> egui::Id {
    egui::Id::NULL.with("reda-os-allows-motion")
}

/// Keeps what the launch read gave on `ctx`, for [`super::super::set`] to follow while
/// `general.animate` is unset.
pub fn remember(ctx: &egui::Context, allows: bool) {
    ctx.data_mut(|d| d.insert_temp(remembered_id(), allows));
}

/// What the launch read gave; motion is allowed when nothing was read (tests).
pub fn remembered(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(remembered_id()))
        .unwrap_or(true)
}
