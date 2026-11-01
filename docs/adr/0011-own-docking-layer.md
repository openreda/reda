# REDA writes its own docking layer; Documents are ordinary tabs

REDA's main window is laid out by its own docking layer on upstream egui, not by `egui_tiles` or `egui_dock`. The layout model holds tab groups, splits, Docks at the window edges (columns of stacked groups, each collapsible to an icon strip with flyouts) and floating native OS windows. It changes only through layout actions that are checked as a whole before they commit, so a refused move leaves the layout as it was. Each action is a REDA Command (ADR-0001), so layout moves can be scripted, journalled and driven over MCP.

Documents are ordinary tabs. Nothing in the centre is protected: Documents and Panels can go into any group, Documents can be split side by side, and an empty centre is allowed. The default layout puts the Documents in the centre and Projects, Properties and Messages in Docks at the left, right and bottom.

A prototype ([`prototype/main-window-shell`](https://github.com/mnj/REDA/tree/prototype/main-window-shell), crate `prototype-shell-own-dock`) settled this. In about 1,500 lines, it showed drop-target buttons with live previews, rule checks shown before release, collapsible Docks and dragging tabs between OS windows. Neither docking crate offers these. A comparable layer in a reference Rust/egui app suite is about 13k lines plus 12k of tests. It needs no egui patches.

## Considered Options

- **`egui_tiles`** (the earlier pick): rejected. It has no Docks, no icon strips and no drop-target buttons, and it doesn't support native windows or dragging between them. REDA would build those around it anyway.
- **`egui_dock`**: rejected for the same gaps, and it trails egui releases.
- **A protected Document area or canvas**, with Panels only in Docks: rejected. Ordinary tabs make it simple to show several Documents side by side, and the extra rules gave the user nothing they wanted.

## Consequences

- REDA owns and tests a sizeable UI subsystem. It should live in its own module or crate under `reda-ui` and be tested at the model level, through the layout actions, without a window.
- The layout's permanent node and window ids replace `egui_tiles` tile ids as the targets of layout Commands.
- Saving and restoring layouts, keyboard docking and accessibility are REDA's to build.
- How floating windows behave on Wayland, which reports no window positions, is still open.
