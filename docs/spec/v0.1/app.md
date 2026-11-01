# v0.1 application

The window, docking, canvas rendering, the Keymap and icons, Preferences and per-user state, crash recovery, i18n and accessibility. Crates: `reda-ui` and `reda-app`, plus `reda-render` for the canvas.

## Window shell

Sources: [How should REDA's main window shell look and behave?](https://github.com/openreda/reda/issues/17), [Should REDA write its own docking layer instead of egui_tiles?](https://github.com/openreda/reda/issues/21), [ADR-0011](../../adr/0011-own-docking-layer.md), [How should floating windows behave on Wayland?](https://github.com/openreda/reda/issues/22).

- **One window, one Project per instance.**
  - Opening another Project replaces the open one, or starts a second instance.
  - With no Project open, the Document area shows a **Start page** tab. It lists recent Projects (missing ones greyed out, each with a remove button) and offers New Project and Open Project.
  - The Start page is an ordinary tab, and it closes when a Project opens.
- **Docking layer:**
  - REDA's own docking layer on upstream egui (ADR-0011). It does not use `egui_tiles` or `egui_dock`.
  - The model holds tab groups, splits, **Docks** at the window edges and native OS windows. A Dock is a column of stacked groups that collapses to an icon strip with flyouts.
  - The layout changes only through layout actions, and each action is a Command. Each action is checked as a whole before it commits, so a refused move leaves the layout as it was.
  - Dragging a tab shows drop-target buttons with a live preview, and any rule breach is shown before the release.
- **Documents are ordinary tabs:**
  - Nothing in the centre is protected. Documents and Panels can go into any group.
  - Documents can be split side by side, and an empty centre is allowed.
- **Default Layout:**
  - Documents in the centre.
  - **Projects** in a left Dock, **Properties** in a right Dock and **Messages** in a bottom Dock.
  - Other Panels: Library, Layers and Journal.
  - A status-bar **Panels** button reopens closed Panels.
- **Native windows:**
  - A tab moves into an OS window through its tab menu, or by dragging it out of the main window.
  - Tabs can be dragged between windows.
  - On X11, Windows and macOS, a drop is hit-tested by screen position, with drop buttons and a preview in the target window.
  - **On Wayland** the drop is blind. After a release outside the source window, the first REDA window that sees the pointer takes the tab, at the drop target under the pointer (or at its first group). If no window sees the pointer within about 0.7 s, the tab floats in a new window. There is no cross-window preview, and the compositor places new windows.
- **Menus and toolbars:**
  - Menus and toolbars follow the active Document, which is the last one interacted with in any window.
  - An **Active Bar** floats over each canvas with the most-used Tools for that Document type.
  - Menus, toolbars and the Active Bar are data files in `reda-ui`, listed per Document type. They have stable ids and bind only to Commands, Presets, raw `{command, params}`, separators or submenus. There are no hard-coded callbacks, and a test enforces this. They can't be customised in v0.1 ([customisation](https://github.com/openreda/reda/issues/35)).
- **Theme:**
  - REDA's own dark and light themes, not stock egui. They use neutral greys, one blue accent, small corner radii, no shadows, tighter spacing and slightly smaller text.
  - Both themes meet WCAG AA text contrast.
  - Default: follow the OS.
- **Status bar:** the "out of sync: N changes" badge, the bridge indicator, the Panels button, and notes such as "Zones need refilling".
- **Command Palette** (`Mod+Shift+P`):
  - It does a fuzzy search over every Command and Preset in the current context.
  - It shows each shortcut, and the reason for any that are unavailable.

## Canvas rendering

Sources: [ADR-0010](../../adr/0010-canvas-drawn-by-own-wgpu-renderer.md), [How are text, Zone fills, highlighting and layer colours drawn on the canvas?](https://github.com/openreda/reda/issues/26), [prototype research](https://github.com/openreda/reda/issues/18).

- **Scene:**
  - Each Document is turned into a GPU-free **Scene**: a list of primitives (capsule, rect, ring, triangle lists, glyph runs), each tagged with its object UUID and Layer.
  - The schematic and PCB canvases and `document.render` (PNG) draw the Scene with the **same wgpu backend**, embedded in egui through an `egui_wgpu` paint callback.
  - PDF and SVG are written from the same Scene.
- **Uploads:**
  - Geometry is uploaded once, as instanced SDF primitives with analytic anti-aliasing, and re-uploaded only when the Document changes.
  - Each frame writes only the view transform.
- **Render target:**
  - The canvas renders into its own offscreen texture with 4× MSAA, which is then copied into the window.
  - When nothing has changed, the last frame is reused.
- **Layer compositing:**
  - Each Layer is drawn opaque into a target, then blended at the Layer's opacity.
  - Layers are drawn bottom to top in physical order, with the active Layer last.
  - A key cycles inactive Layers through Normal, Dimmed and Hidden (single-Layer mode).
  - The view-from-bottom flip mirrors the view and reverses the order.
  - The schematic is one Layer at full opacity.
- **Colours:**
  - Colour schemes are keyed by Layer kind and side, and by schematic object kind, never by Layer UUID.
  - One fixed scheme ships per theme, and the schematic sheet follows the theme.
  - Colour overrides are Preferences.
- **Zones:**
  - The outline is always drawn.
  - Fill results are triangulated (`i_triangle`) once, when they arrive.
  - A stale fill stays visible, hatched or dimmed, with a "Zones need refilling" indicator.
  - A Zone that has never been filled shows only its outline.
- **Highlighting:**
  - Hit-testing runs on the CPU, against the `rstar` index in the domain crates.
  - A per-instance flag buffer, separate from the geometry, marks objects as:
    - selected: lightened, with a halo
    - hovered: a faint lighten
    - Net-highlighted or cross-probed: everything else is dimmed
    - dimmed
  - Changing the selection rewrites only the flags.
- **Live interaction:**
  - Drags, rotates and placements in flight apply a per-frame transform to the flagged instances.
  - These go in a transient overlay buffer: rubber-banded Wires, the routing Track, the selection rectangle, Violation markers and the snap cursor.
  - The grid is drawn by a shader.
  - egui draws only UI on top: the Active Bar and tooltips.
- **Text:**
  - Document text is always drawn.
  - View-only labels (Pad numbers, Pin numbers, Net names on Pads and Tracks) are generated by the renderer. Each has a minimum on-screen size, below which it is hidden.
- **Fonts:**
  - PCB text defaults to Hershey Roman Simplex. Schematic text defaults to bundled **Noto Sans**.
  - Either font can be chosen on any text object.
  - `reda-app` finds OS-installed fonts and hands their bytes to the engine.

## Keymap

Sources: [How do Commands get their labels, icons and shortcuts?](https://github.com/openreda/reda/issues/27), [How far does Keymap and menu customisation go in v0.1?](https://github.com/openreda/reda/issues/35).

- **What a binding names:**
  - Shortcuts, menus, toolbars, the Active Bar and the Palette bind to a Command or a Preset.
  - A user binding may also name a raw `{command, params}`. Its label is the Command title plus a short summary of the params.
- **The default Keymap:**
  - It is a data file in `reda-ui`, in the same format any later Keymap preset will use.
  - A Preference `keymap.base` picks the base Keymap. It is only `"reda"` in v0.1, and may be hidden.
- **Contexts:**
  - The most specific context wins: **active Tool → Document type (schematic / pcb / symbol / footprint) → global**.
  - Text fields and dialogs take keys first.
  - A key binds only once per context. A clash is a load error in Messages, and the earlier binding is kept.
- **Default scheme.** The exact letters can change during the build, but the scheme is fixed:
  - **File and edit:** OS-standard shortcuts with `Mod` (Cmd on macOS, Ctrl elsewhere): `Mod`+S/Z/Shift+Z/C/V/X/A, and Del.
  - **Tools:** W Wire, N Net Label, P Port, B Power Port, Q No-Connect Marker, T Track, V Via, Z Zone, K Keepout. Esc cancels and returns to select.
  - **In flight:** Space rotate, X/Y mirror, L flip side, Tab Properties, Backspace remove the last segment, `/` corner mode.
  - **View:**
    - G cycles the grid preset, and `Ctrl+Q` toggles mm/mil.
    - `+`/`-` step to the next or previous copper Layer, and `*` swaps top and bottom.
    - `Shift+S` toggles single-Layer mode.
    - F zooms to fit, and Home zooms to the selection.
  - **Move:** M is Move (detach), and D is Drag.
  - **Mouse:**
    - The wheel zooms at the cursor. Shift+wheel scrolls sideways, and Ctrl+wheel scrolls up and down.
    - Right-drag and middle-drag pan. A right-click without dragging opens the context menu.
    - Plain drag keeps connections. `Mod`+drag detaches. `Mod`+click without movement toggles the selection.
  - No multi-key sequences.
- **Customisation:**
  - The user's `keymap.toml` holds only overrides and removals, applied last.
  - Keys are bound by logical key (the character produced) and written portably, such as `"Mod+Shift+Z"`.
  - A **Keyboard Shortcuts** page in Preferences offers:
    - search over every Command and Preset
    - chord recording
    - conflict flags
    - reset to default
    - an "Open folder" link
  - There is no Keymap export or import.

## Icons

Sources: [How do Commands get their labels, icons and shortcuts?](https://github.com/openreda/reda/issues/27), [How should REDA's icons look?](https://github.com/openreda/reda/issues/32). The style guide in that second ticket's resolution is the authority for drawing icons.

- **Icon keys:**
  - Commands, Presets and Tools carry a semantic icon key. `reda-ui` maps each key to one SVG.
  - A test fails when a key has no art.
- **Style:**
  - Line art where colour carries meaning: what already exists is grey, what the action creates is in its role colour, and the points you click are pink dots.
  - Five role colours. Each SVG is drawn with the dark-theme values, which are swapped for the light-theme values on load.
  - SVG on a 24 px grid, rendered through `egui_extras` and resvg.
- **Sizes:**
  - 24 px on toolbars and the Active Bar
  - 20 px in menus
  - 16 px on tabs and Dock strips
- **State:**
  - State shows on the button, never on the icon's colours. Active shows as an accent background, and hover as a hover background.
  - Disabled icons are a faded greyscale copy made on load.
  - Live-state icons, such as the active-Layer swatch, are painted.
- **Interim art:**
  - Until REDA's own MIT set exists, Lucide and Tabler stand in behind the semantic keys. All of them are replaced before the first public release.
  - A small `xtask` vendors the placeholder SVGs (rewriting `currentColor` to `#fff`) into a `reda-ui` icons module and generates the key → `include_image!` table.
  - The set's README records each icon's provenance.
- **Third-party notices:**
  - A `THIRD_PARTY_NOTICES` file is generated at release time. Crate licences come from `cargo-about`, a dev tool. A hand-kept list covers vendored data: placeholder icons, Noto Sans and the Hershey font.
  - It is embedded in the binary and shown under **Help → About → Licences**.
  - A test checks that every vendored asset directory has an entry.

## Preferences

Source: [What per-user state does REDA keep, and where?](https://github.com/openreda/reda/issues/28).

- **The file:**
  - `preferences.toml` holds only the values that differ from the defaults.
  - REDA edits it with `toml_edit`, so user comments survive.
  - The schema comes from `schemars`.
  - An unknown or invalid key gives a warning in Messages and falls back to its default. There is no `format` migration chain, only a small alias table for renamed keys.
- **Commands:**
  - `app.preferences.get` and `app.preferences.set {key, value}`, with keys such as `schematic.repeat_placement`.
  - They are journalled, have no undo, and are reachable from MCP and the CLI.
  - Changes apply at once.
- **Concurrency:**
  - Each write re-reads the file, changes only the touched key, and saves through a temp file and a rename.
  - Other running instances watch the file and reload it.
- **Dialog:**
  - A modal Preferences dialog with a page list, a search box over every setting, and "Reset to default" on each setting.
  - Changes apply live, with no OK or Cancel.
- **v0.1 settings:**
  - **General:** theme dark / light / follow OS (default: follow OS); UI scale; reopen the last Project on start (on); undo depth.
  - **Schematic** and **PCB** (editing):
    - default units mm/mil (mm)
    - repeat placement (on)
    - default snap-grid preset per Document type
    - electrical snap (on)
    - auto-pan at the canvas edge (on)
  - **Display:** Layer colour overrides per theme, keyed by Layer kind (global, not per Project); cursor style (small or full-screen crosshair); default Inactive-Layer mode.
  - **Libraries:** user Library paths.
  - **Fab Presets:** the user's presets.
  - **Agents:** bridge on/off and the allowed agents.
  - **Keyboard Shortcuts:** see [Keymap](#keymap).
- A Project never overrides a Preference.

## Per-user state

- **Location:** `directories::ProjectDirs("com", "openreda", "REDA")`. `REDA_HOME` moves everything under one folder, for tests and portable installs.
- **Config dir:**
  - `preferences.toml` and `keymap.toml`, the only two hand-edited files
  - `layouts/<name>.json`: Saved Layouts
  - `fab-presets/<name>.redafab`
- **Data dir:**
  - `state.json`: machine-written. It holds recent Projects, allowed agents, and the window geometry when no Project is open. It uses read-merge-write.
  - `recovery/`: Recovery Snapshots of library editor tabs when no Project is open.
  - Crash Reports.
- **Runtime dir:**
  - `instances/<pid>.json`, for the bridge.
  - macOS and Windows have no runtime dir, so it falls back to `<cache dir>/instances` with owner-only permissions.
- **Saved Layouts:**
  - Saved Layouts belong to the user. Each one stores Panels by id, and Document *slots*: whether each tab group held Documents, never which ones.
  - `layout.apply:<name>` moves the open Documents into the first Document group and puts each Panel in its saved place.
  - New Projects start from the user's chosen default Saved Layout, or from the shipped default.
  - Floating-window positions are restored where the platform allows. Wayland does not.

## Per-Project view state

- `.reda/view.json` holds:
  - the Layout and the open tabs
  - per Document UUID: zoom and pan, Layer visibility, and the Inactive-Layer mode
- The selection is not saved.
- **Disposable:** if the file is unreadable or was written by another version, it is silently discarded in favour of the default Layout.
- It is saved on close, and debounced on change.
- Viewing never dirties a Document.
- If the last Project is missing, it is not reopened, and a status note says so.

## Crash recovery

Source: [Does v0.1 have autosave and crash recovery, and how is recovery offered?](https://github.com/openreda/reda/issues/33).

- **Recovery Snapshots only.** REDA never saves the real files on its own. Only the GUI app takes snapshots. In-process `reda run` and `reda mcp` never do. Bridged edits land in the app's model, so the app's snapshots cover them.
- **Content:**
  - For each dirty Document, its full canonical JSON goes to `.reda/recovery/<document-uuid>.<ext>`.
  - A dirty `.reda` goes to `.reda/recovery/project.reda`.
  - `manifest.json` records each snapshot's Document path and the hash of the on-disk file it was edited from.
  - Every write goes through a temp file and a rename.
  - The undo stack is not kept.
- **Timing:**
  - A snapshot is written about 5 s after the last change set, and never more than about 60 s apart.
  - Only Documents changed since their last snapshot are written, on a background thread from a read view.
  - The timings are fixed, with no Preference and no off switch.
- **Library editor tabs:**
  - Snapshots go in the open Project's `.reda/recovery/`, or in the per-user data dir when no Project is open.
  - The manifest records the element's absolute path.
  - Starter Library elements never get snapshots.
- **Journal mirror:** the session Journal is appended line by line to `.reda/recovery/journal.jsonl`, as crash evidence that can be replayed.
- **Deleted when:**
  - A Document is saved, or closed with "Don't save": its snapshot is deleted.
  - The Project closes cleanly: everything left is deleted, including the Journal file.
  - So a snapshot left over when a Project opens means the last session ended uncleanly.
- **Recovery on open:**
  - If `instances/` shows a live REDA already holding the Project, REDA hands off to it and offers no recovery.
  - Otherwise, a modal lists each affected Document (and the project file) with its time. Each row offers **Restore** or **Discard**, and the modal also has Restore all and Discard all.
  - **Restore** loads the snapshot as unsaved changes on top of the Document.
  - **Disk changed since the snapshot** (a hash mismatch): the row warns that saving would overwrite newer content, and it defaults to Discard.
  - **Document file deleted or moved:** Restore recreates it as an unsaved Document at the manifest path.
  - **Unparsable snapshot:** shown greyed out with the error. It offers only Discard or "Show in folder", and is never deleted silently.
  - Discarded snapshots are deleted.
- **Crash Reports:**
  - A panic hook writes a local Crash Report (version, OS, backtrace, the tail of the Journal) to the data dir. It makes no emergency save.
  - The next start offers the report with "Open folder" and "Report on GitHub". The GitHub option opens a pre-filled issue in the browser for the user to review.
  - Nothing is ever uploaded automatically.

## Internationalisation

Source: [Is the v0.1 UI built to be translated, and which languages ship first?](https://github.com/openreda/reda/issues/34).

- **v0.1 is English only, but every string has a key:**
  - No user-facing string is a hard-coded literal.
  - Command and Preset titles are looked up by Command id.
  - Violations are a check id plus structured arguments.
  - Other UI text (dialogs, Panels, tooltips, AccessKit names) goes through `tr!("key")`.
- **Catalogue:**
  - The English Fluent (`fluent-bundle`) catalogue lives in `reda-ui`. It may override engine titles with `command-<id>-title`.
  - A test fails when a key is used but is missing from the catalogue.
- **The machine side stays English and untranslated:**
  - Command ids, schemas, titles and descriptions in `docs/commands.json` and over MCP
  - CLI output, the Journal, error results
  - files, logs and Crash Reports
  - user data (Designators, Net names, board text)

  Only `reda-ui` translates.
- **Numbers ignore the OS locale.** `.` is the decimal point, with REDA's unit suffixes (`0.2mm`, `8mil`).

## Accessibility

Source: [How do Commands get their labels, icons and shortcuts?](https://github.com/openreda/reda/issues/27).

- **AccessKit:** every icon-only button, tab and Dock icon has its Command or Panel title as its accessible name, with the shortcut in its description.
- **Focus:**
  - Focus order follows the layout, and focus is always visible.
  - `Ctrl+Tab` / `Ctrl+Shift+Tab` switch tabs in most-recently-used order, through a popup.
  - F6 / Shift+F6 cycle between tab groups and Docks.
- **Keyboard docking:**
  - The tab menu (Shift+F10 or the Menu key) offers:
    - *Move to → Left / Right / Bottom Dock / New window / Split right / Split down*
    - *Close*
    - *Collapse Dock*
  - Each item is a layout Command.
  - There is no keyboard drag mode.
- **Panel Presets:**
  - Every Panel has a show/focus Preset (`panel.show:properties`…).
  - Only Projects, Properties and Messages are bound to keys by default.
- **The canvas** is one accessible element that describes the Document and the selection count.
