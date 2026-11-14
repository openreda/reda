# Chrome inventory: surfaces, widget kinds and styling that bypasses the theme

For [Inventory the chrome surfaces and the styling that bypasses the theme](https://github.com/openreda/reda/issues/270), a ticket of [Map: theme baseline for the application chrome](https://github.com/openreda/reda/issues/266). Taken at `b502fec` (main, 2026-11-14). This branch is throwaway and is never merged.

What is here:

- **This file**: the surfaces and their widget kinds (part 1), and the styling that does not go through `crates/reda-ui/src/theme.rs`, grouped by what it is trying to achieve (part 2).
- **[`chrome-inventory/bypass-sites.txt`](chrome-inventory/bypass-sites.txt)**: every matching call site as `path:line: code`, by category, chrome files apart from on-canvas files. Part 2 summarises it; the build's migration slices can work down it line by line.
- **[`chrome-inventory/before/`](chrome-inventory/before/)**: the before picture, 16 scenes in dark and light at 1600 × 1000 (part 3).
- **`crates/reda-ui/tests/chrome_before.rs`**: the throwaway test that draws the scenes, so the same scenes can be redrawn after the restyle.

## Findings that shape the later tickets

1. **`theme.rs` is small and nearly everything semantic already flows through egui's `Visuals`.** There are 161 direct `ui.visuals()` reads in chrome code against 38 lines with a literal colour or alpha math. The colour migration is modest; the work is in what `Visuals` cannot name.
2. **Sizes are the real bypass.** No spacing, radius, control size or text size has a named token outside the five `TextStyle`s and five `Spacing` fields `theme::style` sets. Chrome code holds 126 `add_space` calls, 31 `Grid::spacing` pairs (12 distinct), 32 literal field widths (13 distinct), 50 painted rects with a literal radius (0, 1, 2, 3, 4 or 5) and about 50 size constants spread over 20 files. Density has nothing to switch today: a Compact mode needs these to become tokens first.
3. **There is no success colour.** The theme has warn and error; the two "OK" ticks paint a literal green.
4. **The tab strip is the largest hand-painted surface** and its colours (blue bars, white underline, white hover wash, amber badge) are fixed hexes shared by both themes, partly in `theme.rs` and partly in `docking.rs`. A quieter tab style replaces all of it in one place.
5. **Three shared building blocks carry most of the chrome**: `chrome::icon_button` (toolbars, Active Bar, Dock strips), `modal::Dialog` (every modal) and the settings shell in `settings/draw.rs` (Project Settings, Board Setup; Preferences has its own near-copy). Floating bars (`Frame::window` + radius 5) and custom popups (`Frame::popup`) are repeated by hand, about 7 and 8 times.
6. **Small glyphs are painted by hand** (chevron, arrow, lock, funnel, tick, cross, close ×) with five different stroke widths, because the UI font lacks them. They are icon candidates for the icons question in the map's fog.
7. **`crates/reda-app` sets no styling at all.** The window is created in `crates/reda-ui/src/app.rs:1176` with native decorations, 1280 × 800, minimum 480 × 320; the UI font is egui's default (no `set_fonts` call anywhere); UI scale is `ctx.set_zoom_factor` from a Preference (`app.rs:725`). `reda-app/src/fonts.rs` indexes installed fonts for PCB text only.

## Part 1: surfaces and widget kinds

The full tables are in [`chrome-inventory/surfaces.md`](chrome-inventory/surfaces.md): about 140 surfaces with their draw function, host and widget kinds; every stock and REDA-drawn widget kind with the surfaces that use it and the states it draws; and the shared building blocks against the one-offs. It was made by reading the code, not by running the app, and its last section lists what was not verified. The shape of it:

| Group | Surfaces | Built from |
|---|---|---|
| **Window shell** | top row (quick strip, main menus, command field, Preferences gear), Command Palette, status bar and its menus, document area and gaps, tab groups and tabs, tab menu, Docks (expanded, collapsed icon strip, flyout), floating windows, tab-drag overlay, tab switcher, Start page, focus ring, tooltips | `egui::Panel::top/bottom/left/right` and `CentralPanel`; `MenuBar`; `Area` + `Frame::popup` for the Palette, flyout and switcher; REDA-painted tabs, strips, gaps, icon buttons and drop targets |
| **Over the canvas** (borderline) | Active Bar and its groups, Routing and Placement Bars, Layer strip and Layer swatch, Show on Board bar, keys card, in-place edit fields, pick list, canvas context menus, grid readout | `Area` + `Frame::window` or `Frame::popup`, each with its own margin and radius; `chrome::icon_button`; REDA Layer chips |
| **Panels** (nine) | Projects, Properties (one mode per selection kind, plus Selection Filter, snap options, rules, overview), Messages, Library, Layers, Journal, Navigator, Find, Pins | stock widgets in a 6 px margin: `CollapsingHeader`, selectable rows, `Checkbox`, `RadioButton`, `ComboBox`, `TextEdit`, hand-built `Grid` tables, plus small painted marks and swatches |
| **Settings windows** (not modal: tabs in a floating window) | Preferences and its pages, Project Settings and its pages, Board Setup and its pages, Output Job, Change List, fab preset pages | a page tree beside a page; four share `SettingsDialog::show`, Preferences has its own copy of the same layout |
| **Modal dialogs** | closing, recovery, crash, new Project, Library dialogs and wizards, schematic dialogs (reannotate, update, replace), paste array, generator and Command params forms, job run report, help and about, updates | all through `modal::Dialog`: a painted backdrop and a stock `egui::Window` |
| **Non-modal tool windows** | Select Similar, Violation details, waive, Auto Wire, Break Wire | stock `egui::Window`, each configured on its own |
| **Element editors** | Symbol, Footprint and Part editors: inspectors, pad form, pin map | stock form rows; the Library preview canvases |

What the widget tables show:

- **Most chrome is stock egui widgets** painted from `theme::visuals`: `Button`, `Checkbox`, `RadioButton`, `ComboBox`, `TextEdit`, `CollapsingHeader`, `ScrollArea`, `Grid`, `Separator`, selectable labels, menu buttons. `Slider`, `DragValue` and `ProgressBar` each appear once or twice. `egui_extras` tables, `Spinner` and `egui::Modal` are not used: every table is a hand-built `Grid` with its own header styling.
- **About 30 widgets are REDA-drawn.** The ones that carry the look: the icon button (`chrome.rs:29`, five states: rest, hover, pressed, selected, disabled), the tab (`docking.rs:723`, three states, literal colours), the Layer chip (`pcb/layer_control.rs:245`, four states) and the drop targets of a tab drag (`docking/drag.rs:459`).
- **Two Active Bar buttons are odd ones out**: the Selection Filter (`selection/filter_ui.rs:46`) and snap palette (`snap/palette.rs:116`) buttons paint their own glyph with only rest and hover, beside icon buttons with five states.
- **Things a modern look might want that do not exist yet**: segmented controls (today rows of `selectable_value`), key-chord caps (chords are weak or monospace text), toasts (notices are status bar text or a dialog), a section-header widget (strong labels or `CollapsingHeader`), draggable splitters between tab groups (splits are equal shares across a 3 px gap; only a Dock's edge drags).
- **Restyled in one place**: everything stock (through `theme.rs`), all modals (`modal::Dialog`), four of five settings shells, fab preset pages, icon buttons and bar separators, menu items (`bars/draw.rs:173`), the tool bars, tab groups and tabs, Layer chips, the focus ring.
- **Rolled separately each time**: the Preferences shell; tree and list rows (five implementations); collapsible sections (two copies plus direct `CollapsingHeader`); form-field rows (about twelve helpers); dialog button rows (inline in almost every dialog, with differing order and spacing); the fifteen floating `Area` + `Frame` surfaces; status marks (five); colour swatches (four); non-modal tool windows (no shared frame).


## Part 2: styling that does not go through `theme.rs`

Counts are lines in chrome files (on-canvas drawing is listed apart in `bypass-sites.txt` and is out of the map's scope). "Through the theme" here means the value comes from `theme.rs`, directly or by way of `ui.visuals()` / `ui.spacing()` / a `TextStyle`.

### 2.1 Colour

| What it is trying to achieve | Where | What it does today | What it needs |
|---|---|---|---|
| **Success / OK mark** | `panels/properties/overview.rs:174`, `panels/properties/rules.rs:122` | literal `rgb(60,170,90)`, same in both themes | a `success` colour beside warn and error |
| **Out-of-sync badge on a tab** | `docking.rs:736-737`, pill at `:812` | dark text `#141619` on amber `#e8a33d` (the dark theme's field and warn hexes, typed again), same in both themes | a badge recipe: fill, text, radius, padding |
| **Tab strip on its blue bar** | `theme::tab_bar`, `tab_text`, `ACTIVE_TAB_FILL`, `ACTIVE_TAB_UNDERLINE` (in `theme.rs`, but fixed hexes outside the `Colors` table); `docking.rs:788` white underline; `:790` white at alpha 20 for hover; `:821` close × at 13 px; reused by the Show on Board bar at `show_on_board.rs:287,306` | the same blues and whites in both themes | tab tokens per theme: bar, active tab, hovered tab, text, close |
| **Dim the window behind a modal** | `modal.rs:13` | black at alpha 100, both themes | a scrim token per theme |
| **Disabled icon** | `chrome.rs:58` (white-alpha tint from `DISABLED_ALPHA`), `icons.rs:96` (`DISABLED_PULL` 0.4 toward grey), `pcb/layer_control.rs:274` | two constants in two files | one disabled recipe |
| **Icon role colours** | `icons.rs` `ROLES` table (dark, light pairs) and `recolour` | the icons' own palette; includes `#3d8fd6`, the dark accent, as the "general" blue | stays its own palette (coloured icons are kept), but the general blue must track whatever accent is picked, and the contrast test on the active button background (`docs/spec/v0.1/app.md`) follows |
| **A drawing accent that should be the theme accent** | `library_dialogs/wizard.rs:427` | the dark accent hex typed in; wrong in the light theme | `theme::accent` |
| **Tints: a quieter version of a semantic colour** | waived or stale message `panels/messages.rs:262` (× 0.5); ERC matrix cell `settings/project/erc.rs:554` (× 0.35); edited rule `panels/properties/rules.rs:260` (× 0.45); refused placement `pcb/placement_bar.rs:144` (× 0.25) and its accent `:149` (× 0.8); drop zone `docking/drag.rs:419` (× 0.2) and mark `:502` (× 0.6); Dock splitter `docking.rs:422` (accent × 0.6) | `gamma_multiply` with eight different factors | named subtle and muted variants of accent, warn and error |
| **Data colours shown in chrome** (not theme colours) | layer swatches `panels/layers.rs:258`, `pcb/layer_control.rs:283`; Net and Net Class swatches `panels/navigator.rs:291,317`; track colour `panels/properties/track.rs:158,230`; stackup materials `settings/board_setup/draw/stackup.rs:113-126`; copper in the Library preview `panels/library/preview.rs:297` | the colour is data; the swatch around it is drawn ad hoc (sizes 10, 12, 18 × 12; radius 0 or 2; outline or none) | one swatch recipe (size, radius, outline); the stackup and preview colours stay local |
| **egui `Visuals` slots used for a meaning they do not name** | `selection.stroke.color` as "accent line" (9 reads: `navigator.rs:532`, `pcb/placement_bar.rs:149`, `settings/board_setup/draw/grid.rs:337`, `selection/picking/pick_list.rs:321`, …); `extreme_bg_color` as "preview well" (5: `panels/library/preview.rs:107`, `library_dialogs/wizard.rs:393`, `panels/properties/rules.rs:233`, …); `faint_bg_color` as "strip" (`docking.rs:443`) and "off cell" (`erc.rs:551`); `widgets.noninteractive.bg_stroke` as "separator" (`chrome.rs:68`, `pcb/layer_control.rs:208`) | works only while the theme keeps those slots equal to those meanings; `selection.stroke` is strong text today, not the accent | named tokens (accent line, well, strip, separator) so egui slots can change freely |
| **Severity colours** | 59 reads of `error_fg_color`, 43 of `warn_fg_color` | through the theme | nothing to migrate; the list is the input for [Where does colour carry meaning in the chrome?](https://github.com/openreda/reda/issues/273) |

### 2.2 Corner radius

Only `theme.rs` knows 3 (widgets, menus) and 4 (windows). Painted chrome types its own:

| Radius | Used for | Where |
|---|---|---|
| 0 | bars, strips, gaps, backdrops, preview frames | `docking.rs:207,443,651,678,679,788`; `modal.rs:64`; `pcb/layer_control.rs:204`; `panels/library/preview.rs:119,121` |
| 1 | check mark box in the Layers panel | `panels/layers.rs:116` |
| 2 | swatches, matrix and grid cells, drop marks, error outlines, the keyboard focus ring | `panels/properties/track.rs:158`; `panels/properties/rules.rs:233`; `settings/project/erc.rs:554,556`; `settings/board_setup/draw/grid.rs:100`; `panels/navigator.rs:291-320`; `pcb/layer_control.rs:283`; `docking/drag.rs:499`; `part_editor/view.rs:551`; `a11y.rs:87` |
| 3 | icon buttons, hovered tab, active tab's top corners, tab badge | `chrome.rs:54`; `docking.rs:777,791,812` |
| 4 | preview wells, drop zone, Output Job badge, the layer strip frame | `panels/library/preview.rs:107`; `library_dialogs/wizard.rs:393`; `docking/drag.rs:416`; `settings/output_job/draw.rs:43`; `bars/draw.rs:661` |
| 5 | floating bars over the canvas | `bars/draw.rs:515,608`; `pcb/layer_control.rs:122`; `show_on_board.rs:297` |

### 2.3 Frames and margins

| What it is trying to achieve | Where | Today |
|---|---|---|
| **A bar floating over the canvas** (Active Bar, placement and routing bars, layer control, Show on Board) | `bars/draw.rs:513,606,659`; `pcb/layer_control.rs:120`; `pcb/routing_bar.rs:164`; `show_on_board.rs:294` | `Frame::window` with its own radius (4 or 5) and inner margin (3, 6, or 6 × 2); offsets `ACTIVE_BAR_TOP` 12, `ACTIVE_BAR_INSET` 8 |
| **A REDA-drawn popup** (Command Palette, tab switcher, keys card, Dock flyout, drag preview, in-place edit fields, canvas pick list) | `palette/draw.rs:115`; `tab_switcher.rs:52`; `keys_card.rs:118`; `docking.rs:638`; `docking/drag.rs:434`; `schematic_editor/decorate.rs:115,166`; `pcb/canvas.rs:334` | `Frame::popup(ui.style())` each time, so they follow the theme's popup look, with no shared helper for size or margin |
| **A grouped block inside a page** | `preferences_dialog/draw.rs:743`; `preferences_dialog/fab_preset_pages.rs:109`; `settings/board_setup/draw/grid.rs:498`; `settings/board_setup/draw/actions.rs:78,140,172`; `settings/project/fab_presets.rs:171` | `Frame::group`, egui's default look |
| **Padding inside a Panel** | `docking.rs:74` (`PANEL_MARGIN` 6), applied at `:708` | one constant |
| **Padding of a header row or pill** | Messages header `panels/messages.rs:126` (6 × 2); Output Job badge `settings/output_job/draw.rs:44` (6 × 1); drag chip `docking/drag.rs:445` | literals |
| **Margin of a modal** | `modal.rs:17` (`EDGE` 8), plus `ui.add_space(resize_corner_size)` at `:80` | one constant |

### 2.4 Spacing

| What it is trying to achieve | Where | Today |
|---|---|---|
| **Vertical rhythm between rows and sections** | 126 `add_space` calls in 44 files | 6 (45 times), 4 (30), 8 (27), 12 (12), then 24, 18, 10 (2 each), 16, 2, 1; no meaning attached to any value |
| **Form grids** | 31 `Grid::spacing([x, y])` calls | 12 distinct pairs: `[12,6]` × 8, `[8,6]`, `[8,4]`, `[12,4]` × 4 each, `[12,8]` × 3, `[14,4]` × 2, and six one-offs |
| **Tighter packing in bars and strips** | `ui.spacing_mut().item_spacing` set to 2 at `docking.rs:469,688`, `pcb/layer_control.rs:214`, and to `chrome::BAR_SPACING` (2) at `bars/draw.rs:231,547,611` | literal 2 in three files |
| **Larger menus** | `bars/draw.rs:93-94`: `override_font_id` 14.5 and `button_padding` 8 × 4 from `chrome::MENU_TEXT` / `MENU_PADDING` | constants in `chrome.rs`; the menus are the one place text is larger than Body |
| **Indent of a tree level** | `preferences_dialog/draw.rs:29` (`INDENT` 14); egui's own indent at `settings/project/schematic.rs:402`, `schematic_dialogs/update/draw.rs:195` | one constant and two defaults |
| **Row height** | `ui.spacing().interact_size.y` read 9 times (`panels/navigator.rs`, `preferences_dialog/draw.rs:154,320`, `settings/draw.rs:291`, `settings/change_list/draw.rs:289`, `app.rs:239` for the status bar) | through the theme: these rows will follow Density for free |

### 2.5 Text size and weight

| What it is trying to achieve | Where | Today |
|---|---|---|
| **Menu text a size up** | `chrome.rs:19` (`MENU_TEXT` 14.5), applied at `bars/draw.rs:93` | constant outside the theme's text styles |
| **Text painted into a custom widget** | ERC matrix letter `settings/project/erc.rs:568` (11); stackup label `settings/board_setup/draw/stackup.rs:168` (11); generator callouts `library_dialogs/generate/callouts.rs:110` (10); layer control names `pcb/layer_control.rs:253` (12); tab close × `docking.rs:821` (13); pick list glyph `selection/picking/pick_list.rs:337` (14); Library preview pin text `panels/library/preview.rs:165` (computed) | literal `FontId::proportional(n)`: five sizes that match no `TextStyle` (Small is 10.5, Body 12.5) |
| **Text painted through a text style** | tab titles `docking.rs:730` (Button), badge `:741` (Small), drag chip `docking/drag.rs:442` | through the theme |
| **Secondary and emphasised text** | `.small()` 15 sites, `.strong()` 28, `.weak()` 9, monospace 1 (`help/dialogs.rs:83`); `.heading()` is never used | through the theme; `Heading` (16) has no user outside egui's own window titles |

### 2.6 Fixed sizes

| What it is trying to achieve | Where | Today |
|---|---|---|
| **Control and icon sizes** | `chrome.rs:13-24`: bar button 32, quick button 30 with a 22 icon, bar separator 7; `icons.rs:23-27`: icons 24 (toolbar), 20 (menu), 16 (tab); `docking.rs:66-72`: Dock strip 24, strip button 22, tab strip 24, gap 3; `bars/draw.rs:394`: layer strip 24; `docking/drag.rs:25-35`: drop buttons 28 at step 32 | constants in five files; these are the sizes Density would change |
| **Widths of fields** | 32 `desired_width(n)` literals | 13 distinct: 260 × 6, 200 × 5, 240 × 4, 80 × 4, 160 × 3, 180 × 2, 100 × 2, then 60, 70, 90, 120, 140, 480; plus `FIELD_WIDTH` 110 (`pad_form/draw.rs:18`), 220 (`settings/fab_preset/draw.rs:13`), 260 (`part_editor/view.rs:17`), `LENGTH_WIDTH` 90, `THICKNESS_WIDTH` 90, `CELL_WIDTH` 64 |
| **Widths of page trees and label columns** | `settings/draw.rs:16` (`TREE_WIDTH` 150); `preferences_dialog/draw.rs:25-31` (`LIST_WIDTH` 160, `LABEL_WIDTH` 230, `DOT_WIDTH` 14) | Preferences and the settings shell disagree by 10 px |
| **Sizes of popups and windows** | Command Palette 560 wide, list 360 high (`palette/draw.rs:13,144`); command field 360, narrowest 160 (`bars/draw.rs:123-125`); Dock flyout 280 × 320 and floating window 640 × 420 (`docking.rs:76-78`); tab switcher min 240 (`tab_switcher.rs:53`); Start page max 520 (`start_page.rs:26`); Part editor max 720 (`part_editor/view.rs:31`); layer menus min 160 (`pcb/layer_control.rs:134,173`) | literals |
| **Heights of scrolling lists** | 17 `max_height(n)` calls: 120, 200, 220 × 2, 240, 320 × 3, 360, 380 and computed | literals |
| **Hand-painted glyph boxes** | 26 `allocate_exact_size` calls: 12 × 12 marks, 14 × 12 arrow, 16 × 20 lock, swatches | literals |

### 2.7 Stroke widths and hand-painted glyphs

| What it is trying to achieve | Where | Today |
|---|---|---|
| **Flag a field as invalid** | `settings/draw.rs:91,93`; `settings/board_setup/draw/grid.rs:138`; `part_editor/view.rs:554` | error colour at 1.5, set three separate ways (two by mutating `visuals_mut()` in a scope, one by painting a rect) |
| **Keyboard focus ring** | `a11y.rs:87-90` | accent at 2.0, radius 2, drawn in its own foreground Area |
| **Separators** | `chrome.rs:68`; `pcb/layer_control.rs:208` | 1.0 in the noninteractive stroke colour |
| **Glyphs the font lacks** | chevron `docking.rs:1201` (1.5); arrow `settings/change_list/draw.rs:319` (1.2, 1.5); lock `selection/picking/pick_list.rs:150` (1.5); funnel `selection/filter_ui.rs:70` (1.5); tick and cross `panels/properties/overview.rs:174,187`, `panels/properties/rules.rs:122-138` (2.0); sort and drag marks `panels/navigator.rs:532,567`, `settings/board_setup/draw/grid.rs:104,337` | painted line by line; widths 1.0, 1.2, 1.3, 1.5 and 2.0 |
| **Drop-zone and preview outlines** | `docking/drag.rs:420,450,473`; `pcb/placement_bar.rs:147,164` | 1.0 and 2.0 |

### 2.8 Already central, but not in `theme.rs`

These are single definitions, so they are cheap to move, but today they are not tokens a Density or theme can switch: `chrome.rs` (bar button, quick button, menu text and padding, bar spacing, separator), `icons.rs` (icon sizes, role colours, disabled pull), `docking.rs` (strip, tab strip, gap, panel margin, flyout and window sizes), `modal.rs` (backdrop, edge), `bars/draw.rs` (Active Bar offsets, layer strip height). `theme.rs` itself keeps `DISABLED_ALPHA`, the tab colours and the active-tab fill and underline as free constants outside the per-theme `Colors` table.

## Part 3: the before picture

[`chrome-inventory/before/`](chrome-inventory/before/) holds each scene as `<id>-dark.png` and `<id>-light.png`, 1600 × 1000, drawn by the real window through wgpu on Mesa's llvmpipe at `b502fec`, with `manifest.json` listing them.

| Id | Scene |
|---|---|
| `01-start` | Start page with recent Projects |
| `02-schematic` | Schematic editor: Projects, Properties and Messages Docks, Active Bar, status bar |
| `03-pcb` | PCB editor with the Layers panel and Layer strip |
| `04-menu` | the View menu open |
| `05-palette` | Command Palette with a query |
| `06-tab-menu` | a tab's context menu with its Move to submenu |
| `07-collapsed-dock` | a collapsed Dock's icon strip with a flyout open |
| `08-preferences` | Preferences, General page |
| `09-project-settings` | Project Settings, ERC page with the pin matrix |
| `10-board-setup` | Board Setup, Net Classes grid |
| `11-output-job` | Output Job after a run |
| `12-library` | Library panel and Part editor |
| `13-messages` | Messages panel after ERC, with a Violation marked |
| `14-focus-tab-switcher` | keyboard focus ring and the tab switcher |
| `15-params-form` | a Command's params form, a modal with its backdrop |
| `16-tooltip` | a Tool's name-and-key tooltip |

To redraw them (for the after picture, or after adding a scene to `crates/reda-ui/tests/chrome_before.rs`):

```sh
REDA_SHOWCASE_OUT=$PWD/docs/research/chrome-inventory/before REDA_PNG_SNAPSHOTS=1 \
LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl \
cargo test --locked -p reda-ui --test chrome_before -- --exact every_scene_renders_in_both_themes
```

The 32 dark-only snapshots in `crates/reda-ui/tests/snapshots/` (900 × 560) cover more Board Setup pages, the editors and the tab drag, and will need rebaselining in the build. Not captured here: the closing, recovery and crash dialogs, the Library wizards, the Symbol and Footprint editors, and the Routing and Placement Bars.

