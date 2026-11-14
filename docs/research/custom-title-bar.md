# Research: a custom title bar on Wayland, Windows and macOS with winit 0.30.13 and eframe 0.37

For [#269](https://github.com/openreda/reda/issues/269), a child of the theme-baseline map [#266](https://github.com/openreda/reda/issues/266). Researched 2026-11-14.

**Method.** API names and behaviour were read from the crate sources at the versions pinned in `Cargo.lock`: `eframe`, `egui`, `egui-winit`, `egui-wgpu` 0.37.0; `winit` 0.30.13; `sctk-adwaita` 0.10.1; `smithay-client-toolkit` 0.19.2 (the one winit links). Source references below are `crate-version/path:line`. Protocol text is from wayland.app, platform rules from Microsoft Learn and Apple's AppKit reference, compositor behaviour from the compositors' own source. **Nothing was run on a real GNOME, KDE, sway, Windows or macOS session**; claims that come from reading code rather than from documentation are marked *(read from source)*, and anything not checked against a primary source is listed under [Not verified](#not-verified).

## Short answer

| | Out of the box with eframe 0.37 | REDA code needed | Blocked by upstream |
|---|---|---|---|
| **macOS** | Content under a transparent title bar, native traffic lights kept and repositioned, native resize, shadow, corners, full screen | Leave a left inset for the traffic lights; drag and double-click on the bar | Repositioning traffic lights in floating windows (the API reaches the main window only) |
| **Windows** | Frame removed, DWM shadow kept, move with Aero snap, edge resize by command, maximise clamped to the work area | Own minimise/maximise/close buttons, edge-resize hit strip, double-click to maximise | Snap layouts flyout on the maximise button ([winit#3884](https://github.com/rust-windowing/winit/issues/3884), open); system menu on right-click in floating windows |
| **Wayland** | Decorations off, interactive move and resize, maximise, minimise, maximised and full-screen state | Own buttons, edge-resize hit strip with cursors, double-click, rounded corners on a transparent surface, hiding the controls when the compositor insists on drawing its own | Tiled state and window geometry ([winit#4812](https://github.com/rust-windowing/winit/issues/4812), open), so no client-drawn shadow and no squaring of corners when tiled; compositor capabilities |

Cost and risk, lowest first: **macOS** (small, low risk), **Windows** (medium; one visible gap, snap layouts), **Wayland** (largest; loses the window shadow REDA has today on GNOME, and tiling compositors may draw a second title bar). Detail in [Cost and risk](#cost-and-risk-per-platform).

## What REDA does today

- Both window kinds use the OS title bar. The main window is built in `crates/reda-ui/src/app.rs` (`run`, `ViewportBuilder::default().with_title(..).with_inner_size(..).with_min_inner_size(..)`), and each floating window of the docking layer (ADR-0011) in `crates/reda-ui/src/docking.rs` (`native_window`, `ctx.show_viewport_immediate` with its own `ViewportBuilder`). Neither sets `decorations`, `transparent` or any macOS title-bar option.
- `docs/spec/v0.1/app.md#window-shell`: "Title bars are the OS's. Where the compositor draws none, as GNOME does on Wayland, REDA uses winit's Adwaita title bar".
- eframe depends on winit with default features off, which drops `wayland-csd-adwaita`. The workspace `Cargo.toml` turns it back on (`winit = { version = "0.30.13", default-features = false, features = ["wayland-csd-adwaita"] }`), `crates/reda-app` depends on winit only for that feature, and `crates/reda-app/tests/title_bar.rs` asserts `sctk-adwaita` with `ab_glyph` is in the tree. `deny.toml` ignores RUSTSEC-2026-0192 (unmaintained `ttf-parser`) because of that path.
- That Adwaita frame (`sctk-adwaita-0.10.1/src/lib.rs`) is what gives GNOME users a title, buttons, resize borders, a drop shadow (`shadow.rs`, skipped when tiled, `lib.rs:268`) and rounded corners that go square when maximised or tiled (`lib.rs:686`). A custom title bar replaces all of it, so each of those has to be re-provided or given up.
- The workspace lint `unsafe_code = "forbid"` matters here: any platform work that needs a raw window handle (subclassing a Windows window procedure, touching `NSWindow`) cannot be written in REDA's own crates without an exception or a dependency.

## What egui and eframe 0.37 offer

**Window options** (`egui-0.37.0/src/viewport.rs`, struct `ViewportBuilder`, applied in `egui-winit-0.37.0/src/lib.rs:2289-2465`):

- All platforms: `with_decorations(bool)`, `with_transparent(bool)`, `with_resizable`, `with_maximized`, `with_fullscreen`.
- macOS only: `with_fullsize_content_view`, `with_titlebar_shown` (false makes the title bar transparent: it maps to winit's `with_titlebar_transparent`), `with_title_shown`, `with_titlebar_buttons_shown`, `with_movable_by_background`, `with_has_shadow`.
- Windows: no option is needed for the shadow. egui-winit sets winit's `with_undecorated_shadow(true)` itself whenever decorations are off and the window is not full screen (`lib.rs:2452`).

**Commands** (`ViewportCommand`, handled in `egui-winit-0.37.0/src/lib.rs:1955-2090`): `StartDrag` (winit `drag_window`, sent only if the window has focus), `BeginResize(ResizeDirection)` (winit `drag_resize_window`), `Maximized(bool)`, `Minimized(bool)`, `Fullscreen(bool)`, `Decorations(bool)`, `Close`. There is **no** command for the system window menu and none for corner preference.

**State** (`egui-0.37.0/src/data/input/viewport_info.rs`, `ViewportInfo`): `maximized`, `minimized`, `fullscreen`, `focused`. There is no tiled state and no "is the compositor decorating me" field.

**eframe extras** (`eframe-0.37.0/src/epi.rs`):

- `Frame::set_traffic_lights_position(&self, egui_ctx, title_bar_y: Rangef, left_margin: f32)` centres the native macOS buttons in a custom bar; call it every frame; it leaves them alone in full screen (`src/native/macos.rs`). Added by [egui#8494](https://github.com/emilk/egui/pull/8494).
- `eframe::WindowChromeMetrics::from_window_handle` gives the traffic lights' size.
- `Frame::winit_window()` returns the winit `Window`. Its doc says "the current" window, but the field is set once from the root window (`src/native/epi_integration.rs:239`) *(read from source)*, so it is the **main window only**.
- `App::clear_color` must return transparent for a transparent window; the surface's transparency is taken from the **root** viewport's `transparent` option for every window (`src/native/wgpu_integration.rs:240`, `egui-wgpu-0.37.0/src/winit.rs:286`) and is fixed when the window is created.

**The official example.** [`examples/custom_window_frame` at tag 0.37.0](https://github.com/emilk/egui/blob/0.37.0/examples/custom_window_frame/src/main.rs) is the reference recipe:

- macOS: `.with_fullsize_content_view(true).with_titlebar_shown(false).with_title_shown(false)` and `frame.set_traffic_lights_position(..)`; no buttons of its own.
- Elsewhere: `.with_decorations(false)`, plus `.with_transparent(true)` and a rounded `egui::Frame` for the corners.
- Title bar: an `interact` rect with `Sense::click_and_drag()`; `double_clicked()` toggles `ViewportCommand::Maximized`; `drag_started_by(Primary)` sends `StartDrag`; three egui buttons send `Close`, `Maximized`, `Minimized`.
- It has **no edge resizing**. On Windows and Wayland an undecorated winit window has no resize borders, so the app must add a hit strip that sends `BeginResize`.

**Floating windows.** Options and commands are per viewport, so `docking.rs`'s builder takes the same flags and `ctx.send_viewport_cmd` inside the viewport callback targets that window. What does not reach floating windows is everything hung off `eframe::Frame` (traffic-light positioning, `winit_window()`), because an immediate viewport's callback gets no `Frame` and the `Frame` holds the root window.

## Wayland

### Protocols

- **[xdg-decoration-unstable-v1](https://wayland.app/protocols/xdg-decoration-unstable-v1)**: the client states a preference with `set_mode(client_side | server_side)`; "The compositor can decide not to use the client's mode and enforce a different mode instead", and tells the client the result in `configure`. Without the protocol the client decorates itself.
- **[xdg-shell](https://wayland.app/protocols/xdg-shell)** `xdg_toplevel`: `move` and `resize` start a compositor-driven interactive move or resize and "must be used in response to some sort of user action like a button press"; `show_window_menu` asks for the compositor's window menu, with "no guarantees ... even if a window menu will be drawn at all"; `set_minimized` has no matching state ("There is no way to know if the surface is currently minimized"). The `configure` states include `maximized`, `fullscreen`, `activated`, `tiled_left/right/top/bottom` (since v2) and `constrained_*` (since v7). `wm_capabilities` (since v5) says whether the window menu, maximise, full screen and minimise exist at all.
- **`xdg_surface.set_window_geometry`**: "Client-side decorations often have invisible portions like drop-shadows which should be ignored for the purposes of aligning, placing and constraining windows." A client-drawn shadow must therefore lie outside the window geometry.

### What winit 0.30.13 does *(read from source, `winit-0.30.13/src/platform_impl/linux/wayland/window/`)*

- `with_decorations(false)` creates the toplevel with `WindowDecorations::RequestClient` (`mod.rs:101`), which sctk turns into `set_mode(ClientSide)` when the compositor has xdg-decoration (`smithay-client-toolkit-0.19.2/src/shell/xdg/mod.rs:128`). winit's own frame is created hidden (`state.rs:296`), so it draws no title bar, no borders and no shadow.
- `drag_window` calls `xdg_toplevel.move` and `drag_resize_window` calls `xdg_toplevel.resize`, both with the last pointer-button serial (`state.rs:432-455`). Touch serials are a TODO there, so moving or resizing from a touch screen will not work.
- `set_maximized`, `set_minimized` (un-minimise unsupported), `is_maximized` and `fullscreen` work; `is_minimized` is always `None`.
- With the frame hidden, the window geometry is set to the whole surface (`state.rs:700-712`), and there is no API to set it otherwise.
- The tiled states are read internally (`configure.is_tiled()`, `state.rs:428`) but **not exposed**; neither is `wm_capabilities`. [winit#4812](https://github.com/rust-windowing/winit/issues/4812) (open) asks for both tiled state and surface geometry and has a 0.30-based patch.
- `Window::show_window_menu` is implemented on Wayland (`mod.rs:374`, `state.rs:934`) although its doc comment lists Wayland as unsupported. egui has no command for it, so it is reachable only for the main window, through `Frame::winit_window()`.
- `Window::is_decorated()` returns `true` when the compositor configured server-side decorations and `false` when it accepted client-side with the frame hidden (`state.rs:588`). That is the only way to learn, after the fact, that the compositor is drawing its own title bar; again main window only.

### How the compositors differ

| Compositor | xdg-decoration | What happens when REDA asks for no decorations |
|---|---|---|
| **GNOME (Mutter)** | Not implemented ([wayland.app's support table](https://wayland.app/protocols/xdg-decoration-unstable-v1) marks Mutter unsupported; the request is [mutter#217](https://gitlab.gnome.org/GNOME/mutter/-/issues/217)) | Nothing is drawn around the window. No shadow and no rounded corners unless the client draws them. |
| **KDE (KWin)** | Yes | KWin follows the client's preference: `XdgToplevelWindow::preferredDecorationMode()` returns `Client` for a `ClientSide` request under the default `ClientPreference` policy ([`src/xdgshellwindow.cpp`](https://invent.kde.org/plasma/kwin/-/blob/master/src/xdgshellwindow.cpp)) *(read from source)*. A user window rule can still force a title bar. |
| **sway (wlroots)** | Yes | Only **floating** windows get the client's mode. Tiled windows are always told `server_side` ([`sway/xdg_decoration.c`, `set_xdg_decoration_mode`](https://github.com/swaywm/sway/blob/master/sway/xdg_decoration.c)) *(read from source)*, and sway draws its border and title bar per its `border` setting (default `normal`, which includes a title bar; `sway(5)`). REDA's bar would sit under sway's. |
| **Hyprland** | Yes | Always answers `server_side`, whatever the client asks ([`src/protocols/XDGDecoration.cpp`](https://github.com/hyprwm/Hyprland/blob/main/src/protocols/XDGDecoration.cpp)) *(read from source)*. |

Consequences for REDA:

- **GNOME and KDE** give a clean custom title bar. Both need REDA to draw its own minimise, maximise and close buttons and its own resize strip.
- **Tiling compositors** may answer `server_side`. REDA should then hide its own window buttons (and not round its corners), since the compositor owns the frame. Detection: `is_decorated()` on the main window after the first configure, or a probe before the window is created. Rerun does the latter: it opens a throwaway toplevel with `wayland-client`, asks for `server_side` and reads the answer ([`re_ui/src/wayland.rs`](https://github.com/rerun-io/rerun/blob/main/crates/ui_tools/re_ui/src/wayland.rs)). A probe covers floating windows too; `wayland-client` and `wayland-protocols` are already in REDA's lock file through winit.
- **Rounded corners** are possible: a transparent surface with the corner pixels left clear, as the eframe example does. They must go square when maximised or full screen (both known). They should also go square when tiled, which REDA cannot know with winit 0.30.13; a half-tiled window on GNOME would keep round corners. Today's Adwaita frame handles that case.
- **Shadow.** A client-drawn shadow needs transparent margin outside the window geometry. winit pins the geometry to the whole surface, so a shadow drawn by REDA would count as window: maximise, tiling and snapping would include it. So with the pinned winit there is **no shadow on GNOME**, which is a regression against the Adwaita frame. Getting it back needs winit#4812 (or a winit fork carrying its patch, which `AGENTS.md` allows for upstream dependencies).
- **Edge resizing.** REDA must reserve a few points along each edge and corner, show the resize cursor, and send `BeginResize` on press. The strip competes with whatever sits at the window edge (Dock splitters, scroll bars).
- **Double-click** is the app's to implement (the example toggles maximise). The compositor's own setting for the title-bar double-click action is not delivered by any protocol.
- **Full screen**: hide the buttons; `ViewportInfo::fullscreen` is reliable.
- If the custom bar becomes the only path, the `wayland-csd-adwaita` feature, `reda-app`'s winit dependency, `tests/title_bar.rs` and the `deny.toml` ignore can all go. If REDA keeps a "use the system title bar" Preference as an escape hatch (Rerun persists such an option), they stay. `ViewportCommand::Decorations` can switch at run time; transparency cannot.

## Windows

### What Microsoft requires

- **Removing the frame**: handle `WM_NCCALCSIZE` with `wParam == TRUE` and return 0, so the whole window is client area; then "you will need to implement logic to handle caption button hit testing and frame resizing/moving" through `WM_NCHITTEST` ([Custom Window Frame Using DWM](https://learn.microsoft.com/en-us/windows/win32/dwm/customframe)).
- **Snap layouts** (the flyout on hovering maximise, Windows 11): a Win32 app with a custom title bar must answer `WM_NCHITTEST` "with a return value of `HTMAXBUTTON` for the maximize/restore button" ([Support snap layouts](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-snap-layout-menu)). The same page asks for a minimum window width of at most 500 effective pixels so the window fits the zones; REDA's 480 passes.
- **Rounded corners** (Windows 11): automatic for windows that keep `WS_THICKFRAME` and `WS_CAPTION` or "a 1-pixel non-client area border"; otherwise opt in with `DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)`. Windows using per-pixel alpha layering or window regions "cannot ever be rounded". Corners are never rounded when maximised or snapped ([Apply rounded corners](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-rounded-corners)).
- **Behaviour and accessibility** ([Title bar design](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design)): 32 px standard height (48 px with a search box); all empty space draggable; double-click toggles maximise; right-click on empty space or the icon shows the system window menu; elements dim when the window is inactive; colours follow light/dark and high-contrast themes; text follows text scaling; own caption buttons should match the system's glyphs and states.

### What winit 0.30.13 does *(read from source, `winit-0.30.13/src/platform_impl/windows/`)*

- Undecorated windows keep `WS_CAPTION | WS_BORDER | WS_SYSMENU` and, when resizable, `WS_SIZEBOX` and `WS_MAXIMIZEBOX` ("Required styles to properly support common window functionality like aero snap", `window_state.rs:257`). The frame is removed in `WM_NCCALCSIZE` (`event_loop.rs:1162`), the documented way.
- With the undecorated shadow on (egui-winit's default), the client rect is shifted by one pixel to keep the DWM shadow, and winit documents the cost: "Enabling the shadow causes a thin 1px line to appear on the top of the window" (`platform/windows.rs:507`).
- Maximised undecorated windows are clamped to the monitor's work area (`event_loop.rs:1171`), so they do not cover the taskbar.
- `drag_window` posts `WM_NCLBUTTONDOWN` with `HTCAPTION` and `drag_resize_window` posts it with `HTLEFT` … `HTBOTTOMRIGHT` (`window.rs:495-545`), so both run the system's own move/size loop. Dragging to a screen edge should therefore snap as usual.
- winit **does not handle `WM_NCHITTEST`** (it appears only in a comment), so the whole window is `HTCLIENT`: no resize borders, and no way to return `HTMAXBUTTON`. [winit#3884](https://github.com/rust-windowing/winit/issues/3884) is open with no workaround in winit.
- `show_window_menu` is implemented with `GetSystemMenu` and `TrackPopupMenu` (`window.rs:560-625`); as on Wayland, egui has no command for it, so main window only. `WS_SYSMENU` is kept, so Alt+Space should still work.
- `with_corner_preference`, `with_border_color`, `with_title_background_color` and `with_system_backdrop` exist in winit (`platform/windows.rs:511-537`) but egui 0.37 does not surface them.

### Consequences for REDA

- Works without platform code: frame off, shadow on, move with Aero snap, `BeginResize`, maximise/minimise/close, maximise respecting the taskbar.
- REDA writes: the three caption buttons, the edge-resize strip (inside the window: native windows have invisible borders outside the frame, undecorated winit windows do not), double-click to maximise, inactive dimming, high-contrast colours.
- **Snap layouts flyout is not available.** The options are: wait for winit#3884; subclass the window procedure from the raw `HWND` (needs `unsafe`, which the workspace forbids, and must be done for every floating window); or overlay a small native child window that answers `HTMAXBUTTON`, the route Tauri plugins take (same `unsafe` problem). Win+Z and drag-to-edge snapping are separate paths that do not depend on the maximise button.
- Right-click system menu: reachable for the main window only.
- The 1 px top line and whether Windows 11 rounds the corners of a winit undecorated window need checking on a real machine. Do **not** copy the example's `with_transparent(true)` blindly on Windows: Microsoft says alpha-layered windows are never rounded by the system (Rerun does request transparency on Windows, "it makes resizing look better", so it evidently works for them; which of the two wins for corners is unverified).

## macOS

### What Apple provides

- [`NSWindow.StyleMask.fullSizeContentView`](https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/fullsizecontentview): "the window's contentView consumes the full size of the window ... respected only for windows with a title bar".
- [`titlebarAppearsTransparent`](https://developer.apple.com/documentation/appkit/nswindow/titlebarappearstransparent): "the title bar does not draw its background, which allows all content underneath it to show through. It only makes sense to set this property ... when fullSizeContentView is also set."
- [`titleVisibility`](https://developer.apple.com/documentation/appkit/nswindow/titlevisibility-swift.property) hides the title text; [`standardWindowButton(_:)`](https://developer.apple.com/documentation/appkit/nswindow/standardwindowbutton(_:)) returns the close, miniaturise and zoom buttons.
- [`performDrag(with:)`](https://developer.apple.com/documentation/appkit/nswindow/performdrag(with:)) hands a mouse-down to the window server as a window drag, which "allows the window to participate in space switching and other system features"; "a mouse-up event may not get sent".

The window stays a normal titled window, so the traffic lights, their hover menu (tiling, full screen), the resize edges, the shadow, the rounded corners and full-screen behaviour are all still AppKit's.

### What winit and eframe do *(read from source)*

- The three `ViewportBuilder` options map to the three AppKit settings above (`winit-0.30.13/src/platform_impl/macos/window_delegate.rs:565-602`). The style mask stays `Titled | Closable | Miniaturizable | Resizable`.
- `with_decorations(false)` is the wrong tool on macOS: it makes a `Borderless` window and loses the traffic lights (`window_delegate.rs:535`).
- `drag_window` calls `performWindowDragWithEvent` (`:1170`). `drag_resize_window` returns `NotSupported` (`:1179`), which does not matter because native edge resizing remains.
- eframe's `set_traffic_lights_position` resizes the private title-bar container views and moves the three buttons each frame (`eframe-0.37.0/src/native/macos.rs`). It reaches into AppKit's view hierarchy, which Apple does not document; that fragility is eframe's to maintain.

### Consequences for REDA

- The main window is the eframe example almost verbatim: three builder options, a left inset for the buttons, `set_traffic_lights_position` each frame, `StartDrag` and double-click on the bar. No buttons to draw, no `unsafe`.
- In native full screen the buttons live in a separate system window, so drop the left inset when `ViewportInfo::fullscreen` is true.
- **Floating windows** get the same builder options, but their traffic lights cannot be repositioned (no `Frame` in the viewport callback), so they stay where AppKit puts them for a standard-height title bar. Either design the floating window's strip around that default position, or get a per-viewport command added upstream.

## Accessibility

- REDA-drawn buttons are egui widgets, so they are exposed through AccessKit like the rest of the UI, provided each has a name (minimise, maximise/restore, close). On macOS the native buttons keep their own accessibility.
- eframe 0.37 creates the AccessKit adapter only for the root window (`init_accesskit` is called once, at `eframe-0.37.0/src/native/wgpu_integration.rs:322`; `initialize_window` for other viewports does not call it) *(read from source)*. So nothing in a floating window is exposed to assistive technology today, custom title bar or not.
- Keyboard window management is the OS's or the compositor's and does not go through the title bar: it keeps working on all three. On Windows the retained `WS_SYSMENU` should keep Alt+Space.
- Microsoft's guidance adds: high-contrast colours, text scaling, inactive state, and a right-click system menu. Pointer targets should be no smaller than the system's (Windows caption buttons are full-height in a 32 px bar).
- Touch: moving and resizing by touch are unimplemented in winit's Wayland backend.

## Other egui apps with a custom title bar

- **Rerun** (the largest shipping egui app, by egui's author's company) is the closest model, and the eframe example and `set_traffic_lights_position` come from that work. [`re_ui/src/lib.rs`](https://github.com/rerun-io/rerun/blob/main/crates/ui_tools/re_ui/src/lib.rs) (`viewport_with_window_chrome`, `custom_window_decorations_default`) on `main` at 330fe94: macOS uses full-size content with native traffic lights; Windows always draws its own decorations; Linux draws its own unless a Wayland probe ([`re_ui/src/wayland.rs`](https://github.com/rerun-io/rerun/blob/main/crates/ui_tools/re_ui/src/wayland.rs)) says the compositor commits to server-side; the choice is a persisted option; transparency is requested wherever custom decorations are possible. The bar itself is in `crates/top/re_viewer/src/ui/top_panel.rs`.
- A GitHub code search for `ViewportCommand::StartDrag` finds many smaller apps that follow the example's recipe, for instance Ferrite (`src/app/title_bar.rs`) and Kaspa NG (`core/src/frame.rs`). These were found by search and not read.

## Cost and risk per platform

| Platform | Cost | Risk | Main reasons |
|---|---|---|---|
| **macOS** | Small | Low | Everything is in eframe 0.37 and proven by Rerun. Open point: floating windows keep the default traffic-light position. Cannot be exercised in CI beyond compiling; needs a manual check on a Mac. |
| **Windows** | Medium | Medium | The bar, buttons, resize strip and state handling are ordinary egui code. The snap layouts flyout is missing until winit#3884 lands or REDA accepts `unsafe` window-procedure code per window. The 1 px top line and corner rounding need a look on Windows 10 and 11. |
| **Wayland** | Large | High | Same egui work as Windows, plus: no shadow on GNOME and no tiled state without a winit change or fork; a second title bar on tiling compositors unless REDA detects server-side mode and hides its buttons; per-compositor differences that can only be found by testing on GNOME, KDE and at least one wlroots compositor. It is the primary platform and the one where today's behaviour (Adwaita frame with shadow) is partly better than what the pinned winit allows. |

Shared work, whatever the platform: one title-bar component used by the main window and by `docking.rs`'s floating windows; the top row (quick strip, menus, command field) would move into or merge with it, with a platform-dependent inset (left on macOS for the traffic lights, right elsewhere for REDA's buttons); and a decision on whether to keep the OS title bar as a Preference.

## Not verified

- Nothing was run. In particular: whether Windows 11 rounds a winit undecorated window's corners, with and without `with_transparent(true)`; how the 1 px top line looks; that drag-to-edge snapping, Win+Z and Alt+Space behave normally on such a window (inferred from the retained window styles and the `HTCAPTION` move loop).
- GNOME's reasons for not implementing xdg-decoration: the mutter#217 page could not be read beyond its title. That Mutter lacks the protocol rests on wayland.app's support table. A comment in Rerun's probe says Mutter advertises the protocol but defaults to client-side, which disagrees with that table; either way the outcome for REDA is the same (the client decorates).
- Whether KWin draws a shadow for a window that chose client-side decorations. KWin `master` has a `DecorationPolicy::Shadow` mode in the same function, which suggests it can; not confirmed.
- Hyprland, niri and COSMIC behaviour beyond the Hyprland source line cited. What Hyprland actually draws after answering `server_side` was not checked.
- Whether GNOME's title-bar button layout (which buttons, which side) can be read by a non-GTK client; sctk-adwaita's handling of it was not read.
- On macOS, whether a click in the transparent title-bar area reaches the egui view in every case, and whether `Maximized` on double-click respects the user's system setting for title-bar double-click. The eframe example and Rerun handle drag and double-click themselves, which suggests events do arrive.
- The compositor and KWin sources were read at their `master`/`main` heads on 2026-11-14, not at a release tag.
