# REDA draws its own title bar

REDA draws its own Title bar on Wayland, Windows and macOS and merges it with the row of menus, so the chrome is one continuous surface and the canvas gains a row of height. Where the pinned winit cannot do what a native title bar does, REDA accepts the gap instead of adding `unsafe` code or forking winit, and keeps the system title bar as a fallback: a Preference, forced on where the desktop insists on drawing its own.

## Considered Options

- **Keep the system title bar everywhere:** no platform work, but on GNOME (the primary platform) winit's grey Adwaita bar sits on top of the themed chrome, and the window spends two rows before the canvas starts.
- **A REDA-drawn strip above the menus, not merged:** matches the colours at the same platform cost and risk, and gives no height back.
- **Close the gaps with `unsafe` window-procedure code on Windows and a winit fork for Wayland:** native fidelity, but it breaks the workspace's no-`unsafe` rule and adds a fork to maintain.

## Consequences

- Windows: hovering the maximise button shows no snap-layouts flyout (winit#3884). Win+arrow keys and drag-to-snap still work.
- GNOME on Wayland: the window has a 1px rim in place of a shadow, and its corners stay round when tiled (winit#4812).
- macOS: the main window keeps native traffic lights, centred in the bar. Floating windows keep AppKit's default button position, so their bar takes the native height.
- Tiling compositors that force their own decorations (sway, Hyprland) are detected before the window is created and get the system title bar path, as does anyone who turns on **Use the system title bar**.
- The fallback keeps winit's Adwaita frame, so the `wayland-csd-adwaita` feature and its advisory ignore stay.
- Revisit the gaps when the two winit issues close.
