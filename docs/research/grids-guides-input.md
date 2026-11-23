# How other EDA tools handle grids, guides, wheel mapping, cursor keys and object defaults

Research for #428, part of map #421 (v0.3 spec). Every source was read on
2026-11-23. The note records what each tool does and where the tools differ. It
recommends nothing for REDA.

A claim marked **unverified** could not be followed to a source that owns it,
and the text says what it rests on. "Not documented" means the pages read do
not say; it does not mean the tool lacks the feature.

## Summary

- **Grids stored with the document.** Only three of the seven tools put grid
  definitions in the design file as first-class data: Altium Designer (a Grid
  Manager list of named grids per board), KiCad nightly (grid items are board
  items, since August 2026) and Horizon EDA (`grid_settings` in the board and
  package files). EAGLE / Fusion stores one grid per drawing file. KiCad 9 and
  10 keep the grid list in user preferences and store only the grid origin in
  the board file.
- **Grid X ≠ Y.** Altium, KiCad, Horizon, DipTrace and OrCAD X all allow it.
  EAGLE's file format has one distance. EasyEDA Pro documents one grid size.
- **Polar grids.** Altium (per-grid polar editor), EasyEDA Pro (a canvas-wide
  switch) and KiCad nightly (polar grid items) have them. KiCad 9 and 10 only
  show polar coordinates in the status bar.
- **Grids scoped to Components or to a region.** Altium scopes a grid by
  extents and by "Comp / Non Comp". KiCad scopes by object kind through "grid
  overrides" in preferences, and KiCad nightly scopes a grid item by area and
  by role (cursor, routing, placement). OrCAD X scopes by layer class (etch
  and non-etch). No other tool surveyed scopes grids.
- **Guides.** Only Altium has placed guides (linear at 0°, 90°, ±45°, and
  point guides) with a Guide Manager. KiCad has had an open request since
  2016.
- **Axis alignment guides while moving.** Altium ("Axes" with an Axis Snap
  Range), KiCad 9 (projection lines from a snap point), EasyEDA Pro
  ("extension" marks, and edge alignment to neighbours) and DipTrace
  ("guidelines") all draw them. They are transient everywhere and stored
  nowhere.
- **Wheel mapping.** Altium's plain wheel scrolls and Ctrl+wheel zooms, with a
  preference table of modifier combinations per action. KiCad's plain wheel
  zooms, with one modifier per action in preferences. EasyEDA Pro offers a
  two-way switch. DipTrace and Fusion zoom on the plain wheel.
- **Arrow keys.** Altium and KiCad move the cursor one grid step, with a
  modifier for ten steps. Altium adds Ctrl+arrow to move the selection.
  DipTrace, EasyEDA Pro and Horizon move the selection with the plain arrows.
- **Object defaults.** Altium has a preference page listing every primitive
  kind, saved in a user-level `.dft` file, with a "Permanent" switch that stops
  Tab edits during placement from changing the defaults. KiCad stores defaults
  per board in Board Setup, by layer class. Fusion has a short per-user list.
  EasyEDA Pro and DipTrace set defaults on the tool while it is active.

## Altium Designer

Altium's documentation is no longer versioned; each page carries an "Updated"
date instead.

### Grids

- **Where they are managed.** Grids are managed in the Grid Manager region of
  the Properties panel, shown when nothing is selected. A default **Global
  Board Snap Grid** always exists. It "cannot be renamed, disabled, or
  deleted", and it covers the whole design space
  ([Working with Grids & Guides](https://www.altium.com/documentation/altium-designer/pcb-working-with-grids-guides),
  updated 2026-10-05).
- **Cartesian grid editor.** Name, unit, rotation about the grid origin,
  Step X and Step Y (linked by default; breaking the link allows X ≠ Y), origin
  X/Y with "Set Origin in PCB View", fine and coarse display as lines, dots or
  "Do Not Draw" with colours, a coarse multiplier of 2×, 5× or 10×, extents
  (width and height of one quadrant) and a choice of quadrants (same page).
- **Polar grid editor.** Name, unit, Angular Step, Radial Step, origin, the
  same display controls, an Angular Range (start and end angle) and a Radial
  Range (min and max) (same page).
- **Region.** The page says grids "can be restricted to a specified area if
  required". The documented controls that bound a grid are its origin, extents
  and quadrants (Cartesian) or its angular and radial range (polar) (same
  page).
- **Priority.** A new local grid gets priority 1 and the others shift down. The
  global grid is fixed at priority 50. The highest-priority grid at the cursor
  applies
  ([same page](https://www.altium.com/documentation/altium-designer/pcb-working-with-grids-guides);
  [Working with the Cursor-Snap System](https://www.altium.com/documentation/altium-designer/pcb-cursor-snap-system),
  updated 2026-10-05).
- **Components only.** Each grid has "Comp" and "Non Comp" checkboxes. Comp
  only: the grid applies during component actions. Non Comp only: it applies to
  all other actions. Both: all objects. Neither: the grid is hidden but still
  applied (Grids & Guides page).
- **Editing shortcuts.** G opens a snap grid menu, Ctrl+G opens the editor of
  the grid under the cursor, and Shift+Ctrl+G sets X and Y of the global grid
  to one value
  ([PCB Editor shortcuts](https://www.altium.com/documentation/altium-designer/shortcut-keys-pcb-editors),
  updated 2026-10-08).
- **Storage.** **Unverified.** None of the pages read says whether grids are
  saved in the PCB document. The Grid Manager sits with board-level properties
  and the global grid is called a *board* snap grid, which suggests they are,
  but no page states it.

### Guides

- **Kinds.** A **Linear Guide** is horizontal, vertical, +45° or −45°; the
  cursor snaps to it where it crosses a grid. A **Point Guide** is a hotspot
  the cursor snaps to when near (Grids & Guides page).
- **Editing.** Guides are placed with a Place button or from Place » Work
  Guides, and added, edited and enabled or disabled in the **Guide Manager**
  region of the Properties panel (same page). Older versions had a Snap Guide
  Manager dialog under Tools; its current page could not be read in detail, so
  its columns are **unverified**.
- **Snapping.** A "Guides" switch in Snap Options turns guide snapping on.
  Guides take priority over the grid, and object hotspots override both
  (Cursor-Snap System page).

### Axis alignment guides

- With "Axes" enabled in Snap Options, the cursor aligns in X or Y with enabled
  object snap points and a dynamic guideline is drawn. **Axis Snap Range** is
  the distance within which the guideline shows; **Snap Distance** also applies.
  It works whenever an edit cursor is shown, "such as during object placement
  or movement" (Cursor-Snap System page).
- The page gives no default for either distance. A user report shows a Snap
  Distance of 8 mil in Altium 21
  ([EEVblog forum, 2021-03-17](https://www.eevblog.com/forum/altium/altium-21-snap-grid-issue/)).

### Wheel mapping

- **Defaults.** Wheel scrolls vertically, Shift+wheel scrolls horizontally,
  Ctrl+wheel zooms, Shift+Ctrl+wheel cycles enabled layers, and Ctrl+wheel
  click opens the Board Insight pop-up (PCB Editor shortcuts page).
- **Editing.** Preferences has a System – Mouse Wheel Configuration page where
  each action takes any combination of Ctrl, Shift and Alt. Actions without a
  named editor apply to all editors except CAMtastic; Launch Board Insight and
  Change Layer are PCB only, Change Channel is schematic only
  ([System – Mouse Wheel Configuration](https://www.altium.com/documentation/altium-designer/system-mouse-wheel-configuration-preferences),
  updated 2026-11-02). The action table on that page is an image, so the full
  action list is **unverified**.
- **Storage.** A preference; the page does not name a file.

### Arrow keys

- Arrow: moves the cursor one snap grid unit. Shift+arrow: ten units.
  Ctrl+arrow: moves the selection one unit. Shift+Ctrl+arrow: moves the
  selection ten units. On a polar grid, Left and Right move inward and outward
  and Up and Down move counter-clockwise and clockwise (PCB Editor shortcuts
  page).

### Object defaults

- **Page.** Preferences » PCB Editor – Defaults lists the primitives, with a
  filter. Selecting one shows its properties for editing
  ([PCB Editor – Defaults](https://www.altium.com/documentation/altium-designer/pcb-editor-defaults-preferences),
  updated 2026-11-02).
- **Permanent.** With Permanent off, properties changed with Tab while an
  object floats on the cursor become the new defaults for that kind. With it
  on, the defaults stay locked (same page).
- **Storage.** "All primitive default values are saved in the ADVPCB.DFT file"
  under the user's `AppData\Roaming\Altium\Altium Designer <GUID>` folder. Save
  As and Load write and read other `.dft` files, and Reset All restores system
  defaults (same page).

### Complaints

- **Snap distance looks like a wrong grid.** A user with a 5 mil grid saw parts
  jump "usually in 10mil steps"; the cause was Snap Distance at 8 mil. A reply
  calls for turning hotspot snapping off with Shift+E, and another notes arrow
  key positioning "sometimes plays weird, especially in SCH" (EEVblog thread
  above).
- Altium's own forum could not be searched without an account, so this is one
  thread, not a survey.

## KiCad

Sources are the 9.0 manual, the `9.0` and `master` branches, and the issue
tracker.

### Grids

- **Storage in 9 and 10.** The grid list, the current grid, the two fast grids
  and the overrides are application settings, per editor, not per project.
  They are parameters under `<window>.grid.*` (`sizes`, `last_size`,
  `fast_grid_1`, `fast_grid_2`, `user_grid_x`, `user_grid_y`,
  `overrides_enabled`, `override_*`)
  ([`common/settings/app_settings.cpp`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/common/settings/app_settings.cpp)).
  The manual says PCB Editor preferences "are stored in the user configuration
  directory"
  ([PCB Editor manual 9.0](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)).
- **Grid origin.** The origin is in the board file, written as
  `(grid_origin x y)`
  ([`pcb_io_kicad_sexpr.cpp`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp)).
  It is set with Place → Grid Origin or typed in Edit → Grid Origin (manual).
- **X ≠ Y.** A grid is a `name`, an `x` and a `y`
  ([`include/settings/grid_settings.h`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/include/settings/grid_settings.h)).
  The manual: grids "can have unequal X and Y spacing as well as an optional
  name".
- **Editing.** A Grids pane in Preferences adds, removes, reorders and edits
  grids and picks two Fast Grids (Alt+1, Alt+2). N and Shift+N step through
  the list (manual).
- **By object kind.** "Grid overrides" set a grid per kind of object, "used
  instead of the default grid when working with those objects", each switched
  on or off, with a global toggle on Ctrl+Shift+G (manual). The stored kinds
  are connected items, wires, vias, text and graphics (`grid_settings.h`).
- **Visible grid.** Dots, lines or crosses, a thickness and a minimum spacing
  in pixels, in Display Options. Snap to grid is "Always", "When grid shown" or
  off (manual).
- **Polar.** In 9 and 10 the only polar feature is a status-bar switch
  "between polar and Cartesian coordinate display" (manual; the
  [10.0 manual](https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html) reads the
  same).

### Grid items in the nightly (milestone 11.0)

- Merge request
  [!2564 "Add PCB_GRIDITEM"](https://gitlab.com/kicad/code/kicad/-/merge_requests/2564),
  merged 2026-08-01, "adds local grids or subgrids as BOARD_ITEM". It closed
  [issue #8252 "Add true polar coordinate system"](https://gitlab.com/kicad/code/kicad/-/issues/8252)
  (opened 2021-04-20, milestone 11.0). A properties dialog and grid-aware
  placing followed on 2026-08-10 (!2746, !2749).
- A grid item has a type (Cartesian or polar), position, orientation, extent,
  spacing (X and Y, or radial and angular), a priority (0 is reserved for the
  global grid; higher wins where items overlap), a major-tick interval, a
  colour and three "affects" flags: cursor, routing and placement. It has no
  layer
  ([`pcbnew/pcb_grid_item.h`, master](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_grid_item.h)).
- The development news thread describes them as "multiple configurable
  sub-grids" with "a priority system to determine the active grid when they
  are overlapped", used for snapping and placement, with routing planned
  ([KiCad forum, 2026-08-17](https://forum.kicad.info/t/post-v10-new-features-and-development-news/66611/29)).
- The master manual does not describe them yet
  ([master manual](https://docs.kicad.org/master/en/pcbnew/pcbnew.html)).

### Guides

- KiCad has no placed guides. A blueprint, "Ability to 'drag in' full
  screen-width (or screen height) guides", was registered on 2016-10-29 and is
  "Not started"
  ([Launchpad](https://blueprints.launchpad.net/kicad/+spec/visual-guides)).
  Whether a GitLab issue replaced it was not checked.

### Axis alignment guides

- When the cursor leaves a snap point, "a horizontal or vertical dashed line
  will appear"; following it keeps the cursor aligned with that point. Line
  segments and arcs also show auxiliary extension lines and circles that can be
  snapped to (9.0 manual, "Snapping to graphical shapes").
- Ctrl suspends grid snapping and Shift suspends object snapping (manual).

### Wheel mapping

- **Defaults.** Plain wheel zooms, Ctrl+wheel pans horizontally and Shift+wheel
  pans vertically: `input.scroll_modifier_zoom` = 0,
  `input.scroll_modifier_pan_h` = Ctrl, `input.scroll_modifier_pan_v` = Shift
  ([`common/settings/common_settings.cpp`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/common/settings/common_settings.cpp)).
- **Editing.** Preferences » Mouse and Touchpad, shared by all KiCad programs:
  one modifier per wheel action, zoom speed, zoom acceleration, centre on zoom,
  reversed directions, horizontal-wheel panning, and what middle and right drag
  do
  ([KiCad manual 9.0](https://docs.kicad.org/9.0/en/kicad/kicad.html)).
- **Storage.** Common user settings, not the project (`common_settings.cpp`).
  An older single "mousewheel pan" switch was migrated to these keys (same
  file).

### Arrow keys

- Arrow keys are the actions Cursor Up/Down/Left/Right. Ctrl+arrow is the
  "Fast" variant and Shift+arrow pans the view
  ([`common/tool/actions.cpp`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/common/tool/actions.cpp)).
- A step is one grid step in that axis, taken from the grid that applies to the
  current selection, and Fast multiplies it by 10
  ([`common/tool/common_tools.cpp`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/common/tool/common_tools.cpp)).
- All are rebindable hotkeys. No default key moves the selection directly.

### Object defaults

- Board Setup » Text & Graphics Defaults sets line thickness, text size and
  text style for six layer classes, plus dimension defaults. Three checkboxes
  say whether the defaults are applied to footprint fields, text and non-copper
  shapes when a footprint is added (9.0 PCB manual).
- These are board design settings, `defaults.*`, alongside defaults for zones
  and pads
  ([`pcbnew/board_design_settings.cpp`, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/pcbnew/board_design_settings.cpp)).
  They belong to the board's project, not to user preferences.

### Complaints

- **Grids are not per project.** A user with two projects keeps re-entering
  Grid 1 and Grid 2 by hand and forgets the values
  ([KiCad forum, 2025-04-03](https://forum.kicad.info/t/request-for-help-with-managing-grid-preferences-across-projects/60092));
  the thread got no reply.
- **Grid origin offsets.** Issue
  [#21535](https://gitlab.com/kicad/code/kicad/-/issues/21535): in 9.0.4 "Grid
  snaps do not follow grid offset if not a grid multiple", so footprints and
  tracks from 9.0.3 no longer sat on the grid.
- **No guides and no polar grid** were long-standing requests (the blueprint
  and #8252 above).

## EAGLE / Fusion Electronics

- **Storage.** An EAGLE drawing file holds at most one `<grid>` element, with
  `distance`, `unitdist`, `unit`, `style` (default "lines"), `multiple`,
  `display`, and an alternate grid `altdistance`, `altunitdist`, `altunit`
  (`<!ELEMENT drawing (settings?, grid?, layers, (library | schematic |
  board))>`, EAGLE 7.1.0 DTD by CadSoft, read from
  [a copy in openpnp](https://github.com/openpnp/openpnp/blob/5bd404cfc70f34103a3ca0fbb6b50c2b465f407c/src/main/resources/eagle.dtd)).
  The format has one distance, so no X ≠ Y, and no polar grid.
- **Fusion.** The API's Grid object "represents the design grid settings for a
  board, schematic, or library", with `distance`, `dots`, `multiple`, `on`,
  `unit` and `unitdist`
  ([Fusion API, Grid Object, preview](https://help.autodesk.com/cloudhelp/ENU/Fusion-360-API/files/electron_Grid.htm)).
- **Preferences.** Per-user preferences hold the grid for *new* schematic and
  2D PCB documents (display, units, Size, Multiple, Alt) and two switches,
  "Reset grid to default when opening schematic documents" and the same for
  board documents. Preferences "are saved with your Autodesk ID"
  ([Electronics preferences](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECAD-PREFERENCES-CPT.htm)).
- **Alt grid.** The alternate grid is active "as long as the key is depressed"
  (same page).
- **Editing.** A Grid dialog on the Design toolbar, and the `GRID` command,
  which can be bound to a key: `ASSIGN CS+G 'GRID MM 0.127 ON;'`
  ([Command line control](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-CMD-CTRL-CPT.htm)).
  The Grid dialog's own help page returned 404, so its fields are
  **unverified**.
- **Guides.** None documented in the pages read. A "Show Inference" preference
  shows temporary alignment graphics on vertices and edges while editing
  sketch geometry (Electronics preferences).
- **Wheel.** "Mouse wheel zoom: Sets the mouse wheel zoom ratio... A value of
  zero turns the option off" (same page). No modifier mapping is documented
  there.
- **Arrow keys.** Not found in the pages read.
- **Object defaults.** A Defaults preference sets "trace width, via diameter
  and drill size, PTH pad diameter and drill size, SMD pad size and roundness,
  polygon isolate and spacing, miter radius, text size, dimension line width,
  extension width, extension length, and extension offset" (same page). It is
  per user.
- **Complaints.** Autodesk forum threads titled "Change Default Grid Size" and
  "Unable to change the default dimensions/units/size/grid in Fusion 360
  Electronics" exist, but the forum refused automated reading (HTTP 403), so
  their content is **unverified**.

## EasyEDA Pro

- **Canvas properties.** With nothing selected, the right panel shows unit,
  grid type, bold grid, grid size, snap size, Alt Snap Size ("When pressing the
  ALT key, the adsorption grid size") and a snap switch. A ratio icon keeps
  grid size and snap size in proportion
  ([Right Panel – Property](https://prodocs.easyeda.com/en/pcb/side-panel-right-property/)).
- **Grid and snap are separate values.** The visible grid size and the snap
  size are two numbers (same page).
- **X ≠ Y.** Not documented; the pages describe a single grid size.
- **Polar.** The PCB editor "Supports switching between two grid types:
  Cartesian coordinate system, polar coordinate system". Polar "takes the
  center of the circle as the origin and expands outward in a fan shape" and is
  "generally used when designing annular boards and layouts"
  ([View – Grid Type](https://prodocs.easyeda.com/en/pcb/view-grid-type/)).
  It is a canvas-wide switch. Its parameters are not documented.
- **Presets.** Common grid sizes, including a polar list, are edited under
  Settings > PCB/Footprint > Common Grid Sizes or View > Grid
  ([View – Grid Size](https://prodocs.easyeda.com/en/pcb/view-grid-size/)).
- **Storage.** Personal settings "are synchronized to the server, no matter
  which browser you log in to"
  ([Settings](https://prodocs.easyeda.com/en/introduction/settings/)). Whether
  the current grid is saved with the PCB document is not documented.
- **Guides.** None documented.
- **Axis alignment.** An "extension" snap option shows marks "When moving the
  mouse to the direction of the line extension or orthogonal position", up to
  six at once, while drawing or moving
  ([Edit – Snap](https://prodocs.easyeda.com/en/pcb/edit-snap/)). A separate
  setting aligns a dragged footprint or pad to the edges of neighbours
  ([PCB Setting](https://prodocs.easyeda.com/en/pcb/settings-pcb/)).
- **Wheel.** "Default mouse wheel zoom, you can modify Ctrl + wheel zoom
  according to personal preference." It is a two-way switch: either the wheel
  zooms and Ctrl+wheel scrolls, or the reverse. Shift+wheel pans horizontally
  in both (Settings page).
- **Arrow keys.** The hotkey table lists the arrows both as "View scroll
  left/right/up/down" and as "Move the selected graph to the left/right/up/
  down"
  ([Hotkeys](https://prodocs.easyeda.com/en/introduction/hotkeys/)). The page
  does not say which applies when. Hotkeys can be reassigned.
- **Object defaults.** In the schematic and symbol editors, Tab while placing
  opens a "Primitive Parameter Preset" for net labels, differential pair flags
  and drawing shapes
  ([Place – Primitive Parameter Preset](https://prodocs.easyeda.com/en/schematic/place-graphic-preset/)).
  No PCB equivalent or defaults page is documented; the PCB settings hold lists
  of common track widths and via sizes (PCB Setting page).
- **Complaints.** A forum user found snap and grid sizes rounded to two
  decimals, so 0.254 mm became 0.25 mm
  ([EasyEDA forum](https://easyeda.com/forum/topic/Grid-size-snap-size-and-others-always-rounding-to-2-decimal-places-420b9c78de284f15a07fcd2de77641d1),
  about 2018, version 5.4.1). That is the older Standard edition, so it may not
  apply to Pro.

## OrCAD X

Cadence's own OrCAD X Presto user guide is rendered by script and its text
could not be read, and the Cadence community forum returned HTTP 403. What
follows rests on a how-to by EMA Design Automation, a Cadence reseller, made
with version 25.1 and updated 2025-12-17
([EMA how-to](https://www.ema-eda.com/how-to-page/how-to-adjust-grids-for-component-placement-in-orcad-x/)).

- **Presto.** A Grids tab turns visibility on and off, offers preconfigured
  X/Y spacings such as "50/250 mils", and has a Quick Grid field: type X and
  "The Y value is automatically populated with the same value". Grid units
  follow the design units. "Grid settings in OrCAD X Presto control both etch
  and non-etch grids."
- **Classic PCB Editor.** Setup > Grids shows separate etch and non-etch
  grids, each with X and Y spacing; the example sets a non-etch grid of X 50,
  Y 250.
- **Snapping.** A Snapping section switches object snapping and picks snap
  targets such as Origin, Center and Grid Points.
- **Unverified, from search snippets of Cadence pages that could not be
  opened:** defaults of 100 × 100 for non-conductor layers and 25 × 25 for
  conductor layers in Presto; a separate grid per conductor layer; an offset
  per grid; grids stored in the board database.
- **Not found:** guides, axis alignment guides, wheel mapping, arrow keys and
  object defaults.

## DipTrace

Source: the complete online help for DipTrace 5.3, updated July 2026
([DipTrace help](https://diptrace.com/support/tutorials/help_html/)), section
numbers given.

- **Grid.** One grid, shown with F11 and snapped to with Alt+F11. The size is
  picked from a list, typed in the toolbar, or stepped with Ctrl+Plus and
  Ctrl+Minus. View \ Customize Grid edits the list, and View \ Precision sets
  a minimum grid size (3.4.11).
- **X ≠ Y.** The PCB Layout View menu has "Y Grid Size – separate grid step
  for the Y axis" (PCB Layout, main menu, View).
- **Per-object snap.** With Snap to Grid off for the project, a single object
  can still be snapped to the grid from its right-click menu. Components align
  "By Pads or By Origin" (Grid Alignment) (3.4.11 and the component submenu).
- **Origin.** A movable design origin, set by mouse or by coordinates, with a
  default position at the sheet centre (3.4.12).
- **Polar, guides, scoped grids.** None documented.
- **Storage.** Not documented.
- **Axis alignment.** While dragging a component or text, "guidelines will
  appear while moving it to help you align it with other components".
- **Wheel.** The wheel zooms; panning is right-drag. No modifier mapping is
  documented.
- **Arrow keys.** "You can also use the Arrow keys" to move selected objects,
  and "Hold Shift for orthogonal moving". While an object is being created the
  arrows pan the design area instead.
- **Object defaults.** No single page. Defaults are set on the Properties tab
  of the Design Manager while the matching tool is active (pad, mounting hole,
  fiducial, static via), in Routing Setup (trace width, clearance and via
  style of the Default net class), and under Objects / Drawing Properties and
  View / Line Width (line width and font, and line width by layer).
- **Complaints.** Not searched.

## Horizon EDA

- **Storage.** The board and the package each serialize `grid_settings`
  ([`src/board/board.cpp`](https://github.com/horizon-eda/horizon/blob/master/src/board/board.cpp),
  [`src/pool/package.cpp`](https://github.com/horizon-eda/horizon/blob/master/src/pool/package.cpp)).
  It holds a `current` grid and a map of named grids by UUID. Each grid has a
  name, a mode (square or rectangular), a spacing and an origin
  ([`src/common/grid_settings.hpp`](https://github.com/horizon-eda/horizon/blob/master/src/common/grid_settings.hpp)).
  The schematic and symbol sources read showed no `grid_settings`.
- **X ≠ Y.** Yes, through the rectangular mode (same header).
- **Editing.** "You can change the size of the grid by changing the value in
  the numeric field on the top left of the window"
  ([Grid docs](https://docs.horizon-eda.org/en/latest/grid.html)).
- **Fine grid.** "By holding Alt you get a finer grid (by a factor of 10)"
  (same page). The modifier is a preference, Alt or Ctrl
  ([`src/imp/imp_key.cpp`](https://github.com/horizon-eda/horizon/blob/master/src/imp/imp_key.cpp)).
- **Visible grid.** "depending on a zoom level your grid may be resized by
  factors of two" (Grid docs).
- **Polar, guides, scoped grids.** None found.
- **Arrow keys.** The plain arrows are bound to move-by-key tools
  (`MOVE_KEY_LEFT` and so on) that move the selection one grid step, and the
  fine modifier with an arrow moves it one fine step (`imp_key.cpp`).
- **Wheel.** Zoom preferences are `smooth_zoom_2d`, `smooth_zoom_3d`,
  `touchpad_pan`, `zoom_factor` and `keyboard_zoom_to_cursor`, plus an
  `invert_zoom` per input device
  ([`src/preferences/preferences.cpp`](https://github.com/horizon-eda/horizon/blob/master/src/preferences/preferences.cpp)).
  No modifier mapping was found.
- **Object defaults.** Not checked.

## Where the tools differ

| | Altium Designer | KiCad 9 / 10 | KiCad nightly | EAGLE / Fusion | EasyEDA Pro | OrCAD X | DipTrace | Horizon EDA |
|---|---|---|---|---|---|---|---|---|
| Grid definitions live in | Board (unverified) | User preferences; origin in board | Grid items in board, plus the preference list | One grid per drawing file; defaults per user | Not documented | Unverified | Not documented | Board and package files |
| Several grids at once | Yes, by priority | No (one active, overrides by kind) | Yes, by priority | No (main + Alt) | No (main + Alt snap) | Etch and non-etch | No | No (one current, named presets) |
| X ≠ Y | Yes | Yes | Yes | No | Not documented | Yes | Yes | Yes |
| Polar | Yes | Status bar only | Yes | No | Yes, canvas-wide | Not found | No | No |
| Scope | Extents; Comp / Non Comp | Object kind | Area; cursor, routing, placement | None | None | Layer class | None | None |
| Placed guides | Linear, point; Guide Manager | No | No | No | No | Not found | No | No |
| Axis guides while moving | Axes + Axis Snap Range | Projection lines | Same | Sketch inference only | Extension marks; edge align | Not found | Guidelines | Not found |
| Plain wheel | Scroll | Zoom | Zoom | Zoom | Zoom (switchable) | Not found | Zoom | Zoom |
| Wheel mapping | Table: any of Ctrl, Shift, Alt per action | One modifier per action | Same | Zoom ratio only | Two-way switch | Not found | Not documented | Not found |
| Plain arrow | Cursor, 1 step | Cursor, 1 step | Same | Not found | Selection or view | Not found | Selection | Selection, 1 step |
| Modified arrow | Shift ×10; Ctrl moves selection | Ctrl ×10; Shift pans | Same | Not found | Not documented | Not found | Shift orthogonal | Fine step |
| Object defaults | Preference page, all kinds, `.dft` file, Permanent | Board Setup, by layer class | Same | Per-user list | Tab preset in schematic | Not found | On the active tool | Not checked |

## Could not be verified

- **Altium:** whether grids and guides are saved in the PCB document; the full
  Mouse Wheel Configuration action table (an image); defaults of Snap Distance
  and Axis Snap Range; the Guide Manager's columns. Altium's own forum was not
  searched.
- **Fusion Electronics:** the Grid dialog help page (404); the Autodesk forum
  threads (403); arrow keys.
- **EasyEDA Pro:** whether the grid is saved in the document; X ≠ Y; polar
  grid parameters; when arrows scroll and when they move the selection.
- **OrCAD X:** everything first-party. The Presto user guide and the Cadence
  forum could not be read; the section rests on a reseller's how-to.
- **DipTrace:** where the grid is stored; any wheel modifier mapping.
- **Horizon EDA:** object defaults; wheel modifiers.
- **KiCad:** whether a GitLab issue for placed guides exists; the grid-item
  feature is unreleased and may change before 11.0.
