# Research: reading the OS reduced-motion setting on Wayland, Windows and macOS

For [#288](https://github.com/openreda/reda/issues/288), a child of the theme-baseline map [#266](https://github.com/openreda/reda/issues/266). Researched 2026-11-14.

**Method.** Crate APIs were read from the sources at the versions pinned in `Cargo.lock`: `egui`, `eframe`, `egui-winit` 0.37.0; `winit` 0.30.13; `zbus` 5.19.0; `windows` 0.62.2; `objc2-app-kit` 0.3.2; `objc2-foundation` 0.3.2; `rfd` 0.17.2. Source references are `crate-version/path:line`. Who pulls which crate and feature was read from `cargo tree -e features -i <crate> --target <triple>`. The portal interface is from the XDG desktop portal documentation, desktop behaviour from the portal backends' and GNOME Settings' own source, Windows and macOS behaviour from Microsoft Learn and Apple's AppKit reference. **Nothing was compiled or run on a GNOME, KDE, Windows or macOS session**; claims that come from reading code are marked *(read from source)*, and what has no primary source behind it is listed under [Not verified](#not-verified).

**Constraints** (ADR-0014, [#274](https://github.com/openreda/reda/issues/274), `AGENTS.md`): no `unsafe` in REDA's crates (`unsafe_code = "forbid"` in the workspace lints), no winit fork, few third-party dependencies, Linux means Wayland.

## Short answer

Yes on all three platforms, without `unsafe` and without adding a crate to `Cargo.lock`. egui, eframe and winit give no help; each platform needs its own small piece of REDA code and a new *direct* dependency on a crate that is already in the tree.

| | How it is read | Cost | Changes observable | From egui/eframe/winit |
|---|---|---|---|---|
| **macOS** | `NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()` | Direct dep on `objc2-app-kit` 0.3 (in the lock through eframe and rfd), one more feature (`NSAccessibility`). Both calls are safe functions. | By polling only. The notification needs `unsafe`. | Nothing |
| **Windows** | WinRT `UISettings::new()?.AnimationsEnabled()` | Direct dep on `windows` 0.62 (in the lock through wgpu-hal), two more features (`UI_ViewManagement`, `Foundation`). Safe functions. The Win32 route, `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)`, is `unsafe` and ruled out. | Yes, `AnimationsEnabledChanged`, Windows 10 2004 and later, safe. Or by polling. | Nothing |
| **Linux (Wayland)** | D-Bus call to the settings portal: `ReadOne("org.freedesktop.appearance", "reduced-motion")`, falling back to `ReadOne("org.gnome.desktop.interface", "enable-animations")` | Direct dep on `zbus` 5 (in the lock through AccessKit), one more feature (`blocking-api`). One or two D-Bus round trips on the session bus. Safe functions. | Yes, the portal's `SettingChanged` signal; needs a listening thread. | Nothing |

Recommendation: **start from it.** In v0.1 the **Animate** Preference takes its initial value from the OS setting when the user has not chosen one, reads it once at startup, and does not track later changes. Detail in [Recommendation](#recommendation).

## What egui, eframe and winit expose

Nothing, on any platform.

- **egui 0.37.0.** A search of `egui`, `eframe` and `egui-winit` 0.37.0 for `reduce motion`, `reduced motion`, `enable-animations`, `CLIENTAREAANIMATION` and `ShouldReduceMotion` finds nothing. The only OS appearance value egui carries is `RawInput::system_theme` (`egui-0.37.0/src/data/input/raw_input.rs:91`). Animation length is an application-set style value, `Style::animation_time`, default 0.2 s (`egui-0.37.0/src/style.rs:318`, `:1550`), which `Context::animate_bool` and its relatives read (`egui-0.37.0/src/context.rs:3709`). That is the switch **Animate** would drive for egui's own transitions; filling it from the OS is the application's job.
- **egui upstream.** No issue or pull request in `emilk/egui` mentions reduced motion (GitHub issue and code search for `reduced motion`, `reduce motion`, `prefers-reduced-motion`, `reduced_motion`, `reduce_motion`: no results). The nearest is the closed [egui#2601](https://github.com/emilk/egui/issues/2601), a question about turning animations off by hand.
- **winit 0.30.13.** The same search finds nothing. winit reads one accessibility setting on Windows, high contrast, for its own theme detection (`winit-0.30.13/src/platform_impl/windows/dark_mode.rs:161`), and handles `WM_SETTINGCHANGE` only to re-derive the theme and send `ThemeChanged` (`src/platform_impl/windows/event_loop.rs:2423`) *(read from source)*. It does not talk to the settings portal at all: on Wayland `Window::theme` "only returns theme overrides" (`src/window.rs:1381`).
- **winit upstream.** The open [winit#1549](https://github.com/rust-windowing/winit/issues/1549) ("Theme mode API") lists "reduce motion" and "disable animations" among settings a future API might cover. Nothing is implemented; code search of the repository for `reduced_motion`, `ReduceMotion` and `enable-animations` returns nothing.
- **AccessKit** (in the tree through eframe's `accesskit` feature): only the iOS adapter mentions the setting (`accesskit_ios-0.2.2/src/adapter.rs:33`). Nothing for the desktop platforms.

## macOS

**The setting.** [`NSWorkspace.accessibilityDisplayShouldReduceMotion`](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayshouldreducemotion), macOS 10.12 and later: "A Boolean value that indicates whether the accessibility option to reduce motion is in an enabled state." Apple's guidance there: "If this property's value is true, avoid large animations, especially those that simulate the third dimension."

**Reading it.** `objc2-app-kit` 0.3.2 binds both steps as safe functions *(read from source)*:

- `pub fn sharedWorkspace() -> Retained<NSWorkspace>` (`objc2-app-kit-0.3.2/src/generated/NSWorkspace.rs:50`)
- `pub fn accessibilityDisplayShouldReduceMotion(&self) -> bool` (`src/generated/NSAccessibility.rs:166`, behind the crate features `NSAccessibility` and `NSWorkspace`)

**Cost.** `objc2-app-kit` 0.3.2 is already in `Cargo.lock`, pulled by eframe and rfd, and `NSWorkspace` is already an enabled feature on macOS; `NSAccessibility` is not. REDA would add a macOS-only direct dependency on `objc2-app-kit` with `features = ["NSWorkspace", "NSAccessibility"]`. No new crate, no `unsafe`, no IPC: the read is one Objective-C message send.

**Observing changes.** Apple: "To receive updates when this setting changes, register for the `accessibilityDisplayOptionsDidChangeNotification` notification using `notificationCenter`", meaning the workspace's own centre; "If you use a different notification center to register, you won't receive the notification" ([notification reference](https://developer.apple.com/documentation/appkit/nsworkspace/accessibilitydisplayoptionsdidchangenotification)). REDA cannot do this without `unsafe`: the notification name is an `extern "C"` static (`NSAccessibility.rs:190`), which Rust only lets you read in an `unsafe` block, and `addObserverForName_object_queue_usingBlock` is a `pub unsafe fn` (`objc2-foundation-0.3.2/src/generated/NSNotification.rs`). What remains is polling the property, for example when the window regains focus; the read is cheap enough for that.

## Windows

**The setting.** Two APIs describe it.

- Win32: [`SystemParametersInfoW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow) with `SPI_GETCLIENTAREAANIMATION` (0x1042): "Determines whether animations are enabled or disabled. The pvParam parameter must point to a BOOL variable that receives TRUE if animations are enabled, or FALSE otherwise." Changes are broadcast as [`WM_SETTINGCHANGE`](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange), whose `wParam` is the `SystemParametersInfo` action.
- WinRT: [`UISettings.AnimationsEnabled`](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.animationsenabled), Windows 10 (10.0.10240) and later: "Gets whether animations are enabled for the user interface." Microsoft's [composition tailoring guide](https://learn.microsoft.com/en-us/windows/apps/develop/composition/composition-tailoring) says applications "should listen and respond to the UISettings.AnimationsEnabled property to ensure animations are turned on or off based on user settings in Settings > Ease of Access > Display". (Windows 11 moved that switch to Settings > Accessibility > Visual effects > Animation effects; the [`SetAnimationsEnabled` reference](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.core.uisettingscontroller.setanimationsenabled) names that path.)

**Reading it without `unsafe`.** The Win32 route is closed: `SystemParametersInfoW` is a `pub unsafe fn` in `windows` 0.62.2 (`src/Windows/Win32/UI/WindowsAndMessaging/mod.rs:2367`) and a raw `extern` function in `windows-sys`. The `WM_SETTINGCHANGE` message is also out of reach, because winit owns the window procedure and passes on only a theme change.

The WinRT route is open. In `windows` 0.62.2 these are safe functions *(read from source, `src/Windows/UI/ViewManagement/mod.rs`)*:

- `UISettings::new() -> windows_core::Result<Self>` (`:1858`)
- `UISettings::AnimationsEnabled(&self) -> windows_core::Result<bool>` (`:1907`)

**Cost.** `windows` 0.62.2 is in `Cargo.lock` on Windows through `wgpu-hal`, with only `Win32_*` features enabled. REDA would add a Windows-only direct dependency on `windows` with `features = ["UI_ViewManagement", "Foundation"]` (`UI_ViewManagement` implies `UI`, which implies `Foundation`). No new crate; more of the `windows` crate gets compiled. The call is a WinRT activation plus one method call, no IPC that REDA has to manage.

**Observing changes.** `UISettings::AnimationsEnabledChanged(handler) -> Result<i64>` (`:2040`) is safe, as is `TypedEventHandler::new` (`src/Windows/Foundation/mod.rs:2467`), which takes a `Fn(..) + Send + 'static` closure. The event exists from Windows 10 version 2004 (10.0.19041) ([event reference](https://learn.microsoft.com/en-us/uwp/api/windows.ui.viewmanagement.uisettings.animationsenabledchanged)); on older builds the binding's cast to `IUISettings6` fails and the call returns an error, which REDA would treat as "cannot observe" *(read from source)*. The `UISettings` object must be kept alive for the handler to stay registered. Polling `AnimationsEnabled` on focus is the alternative.

## Linux (Wayland)

There is no Wayland protocol for this setting; desktops publish it through D-Bus, on the session bus, at `org.freedesktop.portal.Desktop`, object `/org/freedesktop/portal/desktop`, interface [`org.freedesktop.portal.Settings`](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html).

### The interface

- `ReadOne (IN namespace s, IN key s, OUT value v)`: "Reads a single value ... Returns an error on any unknown namespace or key." "This method was added in version 2." The older `Read` is deprecated and wraps the value in a second variant.
- `ReadAll (IN namespaces as, OUT value a{sa{sv}})`.
- Signal `SettingChanged (namespace s, key s, value v)`: "Emitted when a setting changes."
- Standard namespace `org.freedesktop.appearance`, key `reduced-motion` (`u`): "Indicates the system's preferred reduced motion setting. Supported values are: 0: No preference, 1: Reduced motion. Unknown values should be treated as 0 (no preference)."
- "Implementations can provide keys not listed below; they are entirely implementation details that are undocumented." GNOME's `org.gnome.desktop.interface` namespace is one of those.

### Two keys, and which desktops have them

`reduced-motion` is new. It was added to the portal by [xdg-desktop-portal#1840](https://github.com/flatpak/xdg-desktop-portal/pull/1840), merged 2025-11-06 and released in **xdg-desktop-portal 1.21.0** on 2026-01-21 ([NEWS](https://github.com/flatpak/xdg-desktop-portal/blob/main/NEWS.md)). The pull request touched only the two interface description files, so the key's value comes from the desktop's portal backend.

| Desktop | `org.freedesktop.appearance` `reduced-motion` | `org.gnome.desktop.interface` `enable-animations` |
|---|---|---|
| **GNOME 50 and later** | Yes. `xdg-desktop-portal-gnome` maps the GSettings key `org.gnome.desktop.a11y.interface` `reduced-motion` to 0 or 1 and emits `SettingChanged` for it ([`src/settings.c`](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/blob/main/src/settings.c), commit b2dec2ba of 2025-10-24, first tagged in 50.alpha). The GSettings key itself arrived in gsettings-desktop-schemas 50.alpha ("Added accessibility setting for reduced motion"). GNOME Settings' Accessibility > Seeing switch is now **Reduced Motion** and writes that key ([`cc-ua-seeing-page.c`](https://gitlab.gnome.org/GNOME/gnome-control-center/-/blob/main/panels/universal-access/cc-ua-seeing-page.c)) *(read from source)*. | Yes, still served, but the Settings switch no longer writes it. |
| **GNOME 49 and earlier** | No. `ReadOne` returns an error. | Yes. This is the only signal. The Settings switch was **Animation Effects**, bound to `enable-animations` ([`gnome-49` branch](https://gitlab.gnome.org/GNOME/gnome-control-center/-/blob/gnome-49/panels/universal-access/cc-ua-seeing-page.c)) *(read from source)*. The schema describes the key as "Whether animations should be displayed. Note: This is a global key, it changes the behaviour of the window manager, the panel etc." |
| **KDE Plasma** | Yes in current `xdg-desktop-portal-kde`: 1 when `AnimationDurationFactor` in `kdeglobals` is 0 (animation speed set to instant), else 0, with `SettingChanged` on change ([`src/settings.cpp`](https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/blob/master/src/settings.cpp)) *(read from source)*. | No. |
| **Others using `xdg-desktop-portal-gtk`** | Not in its `settings.c` on `main`. | Yes ([`src/settings.c`](https://github.com/flatpak/xdg-desktop-portal-gtk/blob/main/src/settings.c)) *(read from source)*. |
| **No portal running** (bare sway or Hyprland without a backend) | No | No |

Two details of the GNOME backend matter *(read from source)*:

- The `enable-animations` value it serves is not the raw GSettings key. It asks GNOME Shell whether animations are effectively enabled and falls back to the key only when the Shell does not answer (`sync_animations_enabled`), and it emits `SettingChanged` when that effective value changes. So the portal value can be `false` when the user never touched the setting. Which conditions make the Shell report `false` was not checked.
- The two keys mean different things. `enable-animations = false` is "no animations at all", which is exactly what **Animate** off means. `reduced-motion = 1` is the web's `prefers-reduced-motion`: the pull request describes it as "minimising the amount of non-essential motion in animations". REDA's only lever is all or nothing, so both map to **Animate** off.

**Read order for REDA:** `reduced-motion` first (1 means off); if the call errors or returns 0, `enable-animations` (`false` means off); if both error, on.

### Reading it from REDA

No crate in the tree offers this to REDA today. rfd 0.17.2 talks to the file-chooser portal through its own private libdbus binding (`rfd-0.17.2/src/backend/xdg_desktop_portal/portal/`), and `ashpd` is not in `Cargo.lock`. Two routes need no new crate:

1. **`zbus` 5.19.0**, already in `Cargo.lock` on Linux through `accesskit_unix` (eframe's `accesskit` feature), built with `default-features = false, features = ["async-io"]`. Its API is safe Rust. REDA would add a Linux-only direct dependency on `zbus` with `features = ["blocking-api"]`: the synchronous `zbus::blocking` module is gated on that feature (`zbus-5.19.0/src/lib.rs:97`), which is not enabled today and adds no crate (`blocking-api = ["zbus_macros/blocking-api"]`). The read is a session-bus connection plus one or two method calls. It should run off the UI thread, because a call to a portal service that is not running waits on D-Bus activation.
2. **A subprocess**, `gsettings get org.gnome.desktop.interface enable-animations` (and `org.gnome.desktop.a11y.interface reduced-motion` on GNOME 50 and later), with `std::process::Command`. No dependency at all, but GNOME only, it depends on the `gsettings` binary being installed and visible, it reads the raw key instead of the portal's effective value, and it cannot observe changes short of leaving `gsettings monitor` running.

Route 1 is the one that covers GNOME, KDE and the GTK backend with one code path.

### Observing changes

Subscribe to `SettingChanged` on the same proxy and filter for the two keys. All three backends read above emit it. With zbus this needs a thread (or a task on zbus's executor) that waits on the signal stream and hands the new value to the UI with a repaint request.

## Cost summary

| | New crates in `Cargo.lock` | New direct dependency | `unsafe` in REDA | Runtime cost of a read | Observe changes |
|---|---|---|---|---|---|
| macOS | 0 | `objc2-app-kit` (+`NSAccessibility`) | none | one message send | poll only |
| Windows | 0 | `windows` (+`UI_ViewManagement`, `Foundation`) | none | WinRT activation + call | event (2004+) or poll |
| Linux | 0 | `zbus` (+`blocking-api`) | none | session-bus connection + 1 or 2 calls, off the UI thread | signal, needs a thread |

Three target-specific direct dependencies is the real price against "keep third-party dependencies few". Each is a crate REDA already ships, and each replaces code that could not be written in-house without `unsafe`.

## Recommendation

**Start from it; do not follow it in v0.1.**

- **Animate** stays a plain on/off Preference. When the user has never set it, its value is the OS setting, read once at startup: off if macOS reports Reduce Motion, if Windows reports animations disabled, or if the portal reports `reduced-motion = 1` or `enable-animations = false`. If the read fails or the platform gives no answer, on. Once the user flips the Preference, their choice is stored and the OS is no longer consulted.
- **Why not ignore it:** the read is cheap and safe on all three platforms, and a user who has told the OS to stop animating should not have to find a second switch in REDA. On GNOME 49 and earlier and on Windows the OS setting means exactly what **Animate** off means.
- **Why not follow it:** following needs a third state for the Preference ("as the system says"), live observation on three platforms (a D-Bus signal thread on Linux, a WinRT event on Windows, polling on macOS because the notification needs `unsafe`), and an answer for the mismatch on macOS, GNOME 50 and KDE, where the setting means "less motion" and REDA can only offer "none". None of that is needed for a first release, and starting from the OS value leaves the door open: a later version can add the third state without changing what stored `on` and `off` mean.
- **If the Linux cost is unwelcome for v0.1**, macOS and Windows can ship alone (a few lines each); Linux then defaults to on until the zbus read is added. The order of work by cost is macOS, Windows, Linux.

## Not verified

- **Nothing was run.** No read was compiled or executed on any of the three platforms. In particular: that `UISettings::new()` activates in an unpackaged Win32 process on winit's main thread (winit initialises OLE there), and on which thread the `AnimationsEnabledChanged` handler is called.
- **That `UISettings.AnimationsEnabled` and `SPI_GETCLIENTAREAANIMATION` are the same setting.** Microsoft's documentation ties `AnimationsEnabled` to the Settings switch; no Microsoft page found states that it equals the `SPI_` value. Only third-party pull requests say so.
- **macOS threading.** The `objc2-app-kit` signatures take no main-thread marker, and Apple's page states no thread requirement; calling from the UI thread is the safe assumption.
- **Portal front end and backend versions.** Whether an xdg-desktop-portal older than 1.21.0 passes a backend's `reduced-motion` key through was not checked, and neither was the first `xdg-desktop-portal-kde` release that has the key. The GNOME version is inferred from commit dates and tags (commit of 2025-10-24, next tag 50.alpha), not from a release note in `xdg-desktop-portal-gnome`'s NEWS, which does not mention it.
- **GNOME 50 and `enable-animations`.** Whether GNOME Shell or Mutter turns the new `reduced-motion` key into `enable-animations = false`, and under which conditions the Shell reports animations as not effectively enabled, was not read.
- **D-Bus timing.** How long `ReadOne` blocks when no portal is installed (activation failure against the default method timeout) was not measured; hence the advice to keep it off the UI thread.
- **The `gsettings` subprocess route** was not tried, including how it behaves inside a sandboxed package.
- **Compile-time cost** of the extra `windows` features and of `zbus`'s `blocking-api` was not measured.
