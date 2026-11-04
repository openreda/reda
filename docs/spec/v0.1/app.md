# v0.1 application

The window, docking, Projects and settings, canvas rendering, the Keymap and icons, Preferences and per-user state, crash recovery, i18n and accessibility. Crates: `reda-ui` and `reda-app`, plus `reda-render` for the canvas.

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
  - An **Active Bar** floats over each canvas with the most-used Tools for that Document type, about five or six of them. Undo, Redo, the units toggle and the view flip belong on the toolbar, not the Active Bar.
  - The Active Bar sits in a window frame (radius 5, margin 3), 12 px below the canvas top, centred. On a canvas too narrow for it, the Tools that don't fit go into a **More Tools** menu at its end; nothing is clipped.
  - Menus, toolbars and the Active Bar are data files in `reda-ui`, listed per Document type. They have stable ids and bind only to Commands, Presets, raw `{command, params}`, separators or submenus. There are no hard-coded callbacks, and a test enforces this. They can't be customised in v0.1 ([customisation](https://github.com/openreda/reda/issues/35)).
- **Theme:**
  - REDA's own dark and light themes, not stock egui. They use slate greys, one blue accent, small corner radii, no shadows, tighter spacing and slightly smaller text. The values are the ones the [main window prototype](https://github.com/openreda/reda/tree/prototype/main-window-shell) settled:

    | Slot | Dark | Light |
    |---|---|---|
    | Panel | `#1d2024` | `#eceef1` |
    | Window | `#23272c` | `#f7f8fa` |
    | Text field | `#141619` | `#ffffff` |
    | Faint (collapsed Dock icon strips) | `#24282d` | `#e4e7eb` |
    | Widget at rest (checkbox fill) | `#2b3036` | `#dfe3e8` |
    | Hovered | `#343a41` | `#d2d7de` |
    | Pressed | `#3d444c` | `#c5ccd5` |
    | Stroke (outlines, separators) | `#383e45` | `#c3c9d1` |
    | Text | `#c4cad3` | `#2a2f36` |
    | Strong text (hovered, pressed, selected) | `#eef1f4` | `#0d1014` |
    | Weak text | `#949ba5` | `#596069` |
    | Accent | `#3d8fd6` | `#1f6fb8` |
    | Selection and active button | accent at 55 % | accent at 35 % |
    | Link | `#4a98dc` | `#1d68ad` |
    | Gap between groups and Docks | `#141619` | `#c3c9d1` |

    Links are a shade off the accent where the accent itself misses 4.5:1. Text sizes: Body and Button 12.5, Small 10.5, Heading 16, Monospace 11.5. Spacing: button padding 6 × 2, interact height 20, item spacing 6 × 3. Radii: widgets and menus 3, windows 4.
  - Both themes meet WCAG AA text contrast, measured against what shows through a translucent fill.
  - **Buttons** are flat until hovered: no fill at rest, the hover colour when hovered, the selection colour (the accent, translucent, under strong text) when active or toggled. A disabled button fades once, icon and all, to about 47 %. Toolbar and Active Bar buttons are 32 px squares holding a 24 px icon, 2 px apart, with radius 3.
  - **Blue on the active background** ([issue #32](https://github.com/openreda/reda/issues/32)'s follow-up): the icons' general-role blue keeps at least the prototype's contrast on the active button background, 2.0:1 dark and 2.36:1 light. A test enforces this, so a darker or more saturated active colour fails.
  - Default: follow the OS.
- **Tabs and groups:**
  - Each tab group's 24 px tab strip is a solid blue bar across the whole group, the same in both themes. The group holding the active Document, which the menus follow, has the full blue `#1f5fbf`. The other groups have a muted blue: `#2b4f7a` dark, `#3f6593` light.
  - Each tab shows its 16 px icon, its title and any badge. The active tab's title and icon are white, and the other tabs' are `#d6e2f2`. A test keeps both at 4.5:1 or more on both bars. A hovered tab lightens. The out-of-sync badge is a dark-on-amber pill.
  - The active or hovered tab shows a × that closes it.
  - Groups have no borders: they sit 3 px apart, and the gaps show the gap colour. The gap at a Dock's inner edge is its splitter, and it turns the accent (at 60 %) while hovered or dragged.
  - An expanded Dock has no header. It collapses from the View menu's Docks submenu (Collapse Left, Right or Bottom Dock, the `layout.dock.collapse` Presets), or from Collapse Dock on any of its tabs' menus. A collapsed Dock is a 24 px faint icon strip, with no frame around it.
- **Status bar:** from the left, "Active: <Document kind>" (Schematic Sheet, PCB Document, Symbol, Footprint, Part), the cursor's X and Y in the Document's units, and its grid; then notes such as "Zones need refilling", the "out of sync: N changes" badge, the bridge indicator and the Panels button, all flat buttons.
  - The cursor on a board is where it last snapped, measured from the Placement Origin, in the board's units. On a sheet or Library element it is the pointer's last position, in the units of the grid in force. mm read to two decimals, mil to whole mil.
- **Command Palette** (`Mod+Shift+P`):
  - It does a fuzzy search over every Command and Preset in the current context.
  - It shows each shortcut, and the reason for any that are unavailable.

## Projects and settings

Source: [How are Projects, Documents and board setup created and edited in v0.1?](https://github.com/openreda/reda/issues/38).

### New Project

- **One file Command:** `project.new {path, name, fab_preset, board?}`. It works headless and from MCP, and it is one Journal entry. It builds the Project with the same internal builders as `document.add`, `output.job.add` and `pcb.board_outline.add_rect`.
- **What it creates:**
  - the folder `<location>/<name>/` with `<name>.reda`, a first sheet `<name>.redasch` titled "Sheet 1", and `<name>.redapcb`
  - the board's rules and DRC severities, seeded from the Fab Preset
  - one Output Job, named after the Fab Preset
  - All of it is written to disk at once, and the Project opens on the first sheet.
  - A target folder that exists and is not empty is refused.
- **The New Project dialog:**
  - name and location
  - **Fab Preset:** the shipped presets (Generic, JLCPCB, PCBWay) and the user's `.redafab` presets. The default is the last one used (kept in `state.json`), else Generic. The dialog says that the rules and the job can be changed later.
  - **Board:** *Draw later* (the default) or *Rectangle* W × H mm with an optional corner radius. The fields are pre-filled (100 × 80 mm) but used only when *Rectangle* is picked. The rectangle is an ordinary Board Outline polygon that can be edited or replaced later. Later milestones add fields here, such as the layer count in v0.2.

### Documents

- A Document's name is its file stem. Renaming a Document renames its file on save, through a temp file and a rename. The title keeps what the user typed, and the stem is slugified only where the OS requires. The `.reda` entry records both.
- Adding, deleting, renaming and reordering Documents change only the in-memory Project and dirty `.reda`. They reach disk on save, and a deleted Document's file goes to the OS trash.
- These edits are undoable on the Project's own undo stack ([architecture](architecture.md#design-data-model)). Focus picks the stack that `Mod+Z` uses: the Projects panel and the Project dialogs use the Project stack, and a Document tab uses that Document's stack.
- The PCB Document can be deleted and re-added ([PCB](pcb.md#board)).

### Projects panel

```
▾ MyBoard                      (Project Settings…, Save All, Show in folder)
  ▾ Schematic
      Sheet 1 · Power · MCU    (open, rename, reorder by drag, delete, Add sheet)
  ▾ PCB
      MyBoard                  (open, rename, Board Setup…, delete / Add PCB Document)
  ▾ Output Jobs
      JLCPCB                   (Run, Edit…, Duplicate, Delete, New job…)
  ▾ Libraries
      parts/ (Project)         (Add folder…, Remove, Show in folder)
```

- Each item has a dirty marker.
- Double-clicking a Document opens it. Double-clicking a job opens Edit.
- Libraries are browsed in the Library panel. This panel lists only the Project Library paths.

### Settings dialogs

- **Three modal dialogs**, each with a page tree on the left: Project Settings, Board Setup and the Output Job dialog.
- **OK / Cancel / Apply:** each Apply or OK commits the dialog's changes as one `batch` transaction and one undo entry. Invalid values (a negative clearance, a duplicate class name) are flagged inline and disable OK and Apply.
- Preferences stays live with no OK or Cancel, because it is per-user and has no undo ([Preferences](#preferences)).
- They are dialogs, not Document tabs, because none of them is mainly graphical. The Stackup page draws a live cross-section of the dialog's values, and the full Stackup editor in v0.2 is a tab.
- **Project Settings** (stored in `.reda`):
  - *Properties:* title, revision, date, author, company. These are fixed fields only, with no free Project parameters or text variables in v0.1.
  - *ERC:* the severity of each check, with the fixed pin-type matrix shown read-only beside the conflict check ([checks](checks.md#severities)).
  - *Fab Presets:* the Project's copies, each with "Update from source" (after a preview) and "Save as user preset" ([outputs](outputs.md#fab-presets)).
  - *Libraries:* Project Library paths.
- **Board Setup** (stored in the PCB Document):
  - *Rules:* pages Clearance, Track Width, Vias, Holes (size, annular ring and hole-to-hole), Board Edge, Mask & Paste, Silkscreen and Zone Connections.
    - Each page has the board-wide values, an override table with one row per Net Class, the line "Net Class values override the board value", that check's DRC severity, and the [strictest Fab Preset limit](checks.md#fab-presets-and-the-rules) beside each value.
    - Each page has **Reset page to Fab Preset**, the Apply Fab Preset preview filtered to that page.
  - *Mask & Paste:* mask expansion (top and bottom, with a link toggle), from hole edge, Via tenting and paste expansion (absolute or percent). It has no DRC severity ([pcb](pcb.md#mask-and-paste)).
  - *Stackup:* thickness, copper and dielectric entries, finish and colours, with a live cross-section ([pcb](pcb.md#stackup)).
  - *Net Classes:*
    - **Grid:** one bold Default row, which edits the board-wide records, and one row per class in rank order, with a column per value for clearance, Track width and Via size. Drag a row to change its rank. Inherited values are grey and overrides normal, with a "show only overrides" filter, a search filter, an `enabled` checkbox per override and a comment column. Cells copy and paste, and Delete clears an override back to inherited.
    - **Typing in the grid:** typing `min` or `preferred` for a class with no record of that kind fills the record's empty `min`, `preferred` and `max` with the same value. Delete clears any of them.
    - **Net table:** Net → Net Classes, filterable by name, with multi-select, **New Class** from the selection and **Assign by pattern…**. Add, rename and delete classes here.
  - *DRC:* every check's severity, including checks with no rule, such as Courtyard overlap.
  - **Show on board** on each grid and rule row hides the dialog and keeps its pending edits. The objects are highlighted, the canvas is view-only, and a banner gives the count and **Back to Board Setup**.
  - **Undo:** `Mod+Z` inside Board Setup steps back through pending edits. After Apply they are one undo entry.
  - **Dialog actions:** **Apply Fab Preset rules…**, **Save rules as Fab Preset…**, **Export rules…** and **Import rules…** ([checks](checks.md#design-rules)).
- **Output Job dialog:** one job's name, PCB Document, Fab Preset, outputs and destination, plus Run ([outputs](outputs.md#output-jobs)).
- **Shortcuts into the same data:** the Properties panel sets a selected object's Net Classes and [explains its rules](checks.md#explaining-a-rule), and right-clicking a Violation sets that check's severity ([checks](checks.md)).

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
  - The grid is drawn by a shader, as quiet opaque dots (about 1.4:1 on the sheet in either theme). When its dots would come closer than 6 px it coarsens by tens, up to twice, and is hidden after that.
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
  - The most specific context wins: **active Tool → Document type (schematic / pcb / symbol / footprint / part) → global**.
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
  - Six role colours: the five of the style guide, plus a yellow for the Board Outline, Board Cutouts and Keepouts, which are not copper. Each SVG is drawn with the dark-theme values, which are swapped for the light-theme values on load. Every role reaches at least 3:1 on its theme's panel.
  - SVG on a 24 px grid, rendered through `egui_extras` and resvg.
- **Sizes:**
  - 24 px on toolbars and the Active Bar
  - 20 px in menus
  - 16 px on tabs and Dock strips
- **State:**
  - State shows on the button, never on the icon's colours. Active shows as an accent background, and hover as a hover background.
  - Disabled icons are a greyscale copy made on load. The disabled button fades it once, to about 47 %, never twice.
  - Live-state icons, such as the active-Layer swatch, are painted.
- **Art:**
  - Every key has REDA art drawn to the [issue #32](https://github.com/openreda/reda/issues/32) style guide, listed in `icons/reda.txt` with its provenance; a test fails when a key has no art.
  - A small `xtask` generates the key → `include_image!` table and the set's README from `icons/reda.txt`, and fails when a listed key has no SVG.
  - The README records each icon's provenance: who drew it, any reference or generator, and who cleaned it up.
- **Third-party notices:**
  - A `THIRD_PARTY_NOTICES` file is generated at release time. Crate licences come from `cargo-about`, a dev tool. A hand-kept list covers vendored data: Noto Sans, the Hershey font and egui's default UI fonts.
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
  - **General:** theme dark / light / follow OS (default: follow OS); UI scale; reopen the last Project on start (on); undo depth (an optional cap per Document stack; unlimited by default).
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
