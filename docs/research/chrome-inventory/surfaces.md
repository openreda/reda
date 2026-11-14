# REDA chrome surfaces × widget kinds

Inventory of `crates/reda-ui/src` and `crates/reda-app/src` at `b502fec`, made by reading the code; nothing was run.
All paths are relative to `crates/reda-ui/src/` unless they start with `crates/`. `path:line` points at the
`fn` line of the main draw function (or the host call where noted). Each line was read from the file.

How it was made: grep for every host (`egui::Window`, `Area::new`, `Panel::`, `CentralPanel`, `Popup::`,
`MenuBar`, `menu_button`, `context_menu`, `Dialog::new`, `show_viewport`, `Frame::*`), for `allocate_*` /
`painter()` / `interact(`, and a per-file count of widget calls (not kept).
Widget kinds per surface come from that count plus reading the draw function; they were not checked by
running the app.

Facts about the frame that shape everything below:

- egui 0.37 names: the code uses `egui::Panel::top/bottom/left/right` and `egui::CentralPanel`. There is no
  `TopBottomPanel`/`SidePanel` and no `egui::Modal` anywhere.
- Frame order is in `app.rs:210` (`App::ui`): top row, status bar space, docking (Docks, centre, floating
  windows), status bar content, tab switcher, then every dialog, then the focus ring.
- `egui_extras` / `TableBuilder`, `Spinner` and `egui::Modal` are not used. `Slider` appears in two tool
  option windows only; `DragValue` once; `ProgressBar` once.
- `crates/reda-app/src` has no chrome: `main.rs` starts `reda_ui::run`; `fonts.rs` indexes system fonts for
  the engine's text. UI text styles and spacing are set once in `theme.rs:225` (`style`), visuals in
  `theme.rs:160`.

## 1. Surfaces

Kind abbreviations: REDA-drawn = `allocate_*`/`interact` + `painter()`; stock = an egui widget with default
painting from `theme::visuals`.

### 1.1 Window shell

| Surface | Kind | Draw function | Host | Widget kinds |
|---|---|---|---|---|
| Top row (quick strip + menus + command field + gear) | bar | `bars/draw.rs:51` `top_row`; hosted from `app.rs:908` `bars` | `egui::Panel::top("top-row")` (`app.rs:935`) | see next four rows |
| Quick strip (the toolbar, `bars.toolbar`) | bar | `bars/draw.rs:223` `icon_row_sized` | inside top row | REDA icon button (`chrome::icon_button`, 30 px / 22 px icon), REDA bar separator; a Submenu item becomes an icon button opening `Popup::menu`, or `menu_button` when it has no icon; fallback stock `Button` with text when an icon is missing |
| Main menu bar | menu | `bars/draw.rs:98` `menu_bar`, items `bars/draw.rs:173` `menu_items` | `egui::MenuBar` | `menu_button` titles (font and padding overridden by `larger`, `bars/draw.rs:92`), stock `Button` items with image + text, `.selected()` as check state, `.shortcut_text()`, `Separator`, nested `menu_button` submenus; hover text = description, disabled hover text = reason |
| Command field | bar item | `bars/draw.rs:116` `command_field` | `ui.put` in top row | one stock `Button` with weak placeholder text, `extreme_bg_color` fill, weak right-aligned shortcut text, hover text. Not a text field: a click opens the Command Palette |
| Preferences gear | bar item | `bars/draw.rs:153` `gear` | top row, far right | REDA icon button |
| Command Palette | popup | `palette/draw.rs:70` `Palette::show`, rows `palette/draw.rs:180` `draw_row` | `egui::Area` (Order::Foreground) + `Frame::popup` (`palette/draw.rs:111`), placed under the command field | `TextEdit::singleline`, weak "nothing" label, `ScrollArea::vertical`, row = stock `Button` (image + title, weak right text for key/scope), hover text |
| Status bar | bar | `app.rs:1000` `App::status_bar`, segments `status_bar.rs:229` `show` | `egui::Panel::bottom("status-bar")` (`app.rs:240`); content drawn later into the reserved rect | flat stock `Button::frame(false)` segments (`status_bar.rs:217`): kind, cursor X/Y, Delta ΔX/ΔY, grid, snap, tool prompt; `Separator` between; active Layer chip (REDA, `pcb/layer_control.rs:158`); warn-coloured "out of sync" frameless `Button` with hover text; coloured labels; `small_button` (Refill zones, Dismiss); weak "Ready" |
| Status bar: grid menu | popup | `status_bar.rs:289` `grid_menu` | `Popup::menu` on the grid segment (`status_bar.rs:249`) | `selectable_label` rows, `Separator`, `Checkbox` |
| Status bar: active Layer menu | popup | `pcb/layer_control.rs:158` `status_swatch` | `Popup::menu` (`pcb/layer_control.rs:172`) | REDA layer chips |
| Status bar: Panels button | menu | `app.rs:1079` `panels_button` | `ui.menu_button` | stock `Button` items |
| Status bar: agents indicator | menu | `agents.rs:14` `indicator` | `ui.menu_button` | `Checkbox`, `Separator`, labels, `small_button` (Forget), `Button` |
| Status bar message / note ("toast") | notice | `app.rs:1055` `status_message` | inline in status bar | `colored_label` (error colour) or plain label + `small_button` Dismiss. There is no floating toast surface |
| Document area and gaps | background | `docking.rs:182` `show` | `egui::CentralPanel` with `Frame::NONE` (`docking.rs:217`); gap colour painted first (`docking.rs:206`) | REDA: `rect_filled` with `theme::gap`. Splits inside the centre and inside a Dock are fixed equal shares (`docking.rs:1178` `split_rect`, 3 px `GAP`); they have no draggable splitter |
| Tab group (tab strip + body) | tab strip | `docking.rs:670` `group_at` | custom-painted rects inside the centre / Dock / window | REDA: strip `rect_filled(theme::tab_bar)`, body `rect_filled(panel_fill)`; Panels, Start page and settings tabs get `Frame::new().inner_margin(6)`; weak centred label when the group is empty |
| Document / Panel tab | tab | `docking.rs:723` `tab` | REDA-drawn in the strip | REDA tab: silhouette icon tinted to text colour, title galley, amber "out of sync" pill badge, "×" close glyph (own `interact`), hover text on close and badge; drag source for docking |
| Tab menu | menu | `docking.rs:887` `tab_menu`, item `docking.rs:939` | `Popup::context_menu`, or `Popup::from_response(...).kind(Menu)` when opened by key (`docking.rs:872`, `docking.rs:878`) | `menu_button` (Move to), stock `Button` items with disabled hover text |
| Dock, expanded | Dock | `docking.rs:439` `dock`, space from `docking.rs:392` `reserve_dock` | `egui::Panel::left/right/bottom` with `Frame::NONE`, `show_separator_line(false)`, `resizable(true)` | tab groups stacked with `split_rect`; no header. The only splitter is egui's Panel resize edge, restyled locally to a 3 px accent stroke (`docking.rs:422`) |
| Dock, collapsed icon strip | Dock | `docking.rs:467` `icon_strip` | same `Panel`, `resizable(false)`, `exact_size(STRIP + GAP)`; `faint_bg_color` fill painted | REDA chevron button (expand), REDA icon button per tab (22 px), fallback `Button::selectable` with text; hover text |
| Dock flyout | popup | `docking.rs:627` `flyout` | `egui::Area` (Order::Foreground) + `Frame::popup`, fixed 280×320 | strong title label, `Separator`, then the Panel's own content |
| Floating window (native) | floating window | `docking.rs:324` `native_window` | `ctx.show_viewport_immediate` + `CentralPanel` with `Frame::central_panel().inner_margin(0)` (`docking.rs:339`) | tab groups as in the main window; OS title bar |
| Floating window (embedded, where egui cannot open OS windows, e.g. tests) | floating window | `docking.rs:247` | stock `egui::Window` | stock window title bar + tab groups |
| Tab drag: drop overlay and drag preview | drag preview | `docking/drag.rs:383` `overlay`, `docking/drag.rs:459` `draw_button` | own `Ui` on a Foreground layer (`docking/drag.rs:397`) | REDA: translucent drop-preview rect with 2 px stroke, square drop-target buttons with a position mark, title chip following the pointer; refusal reason in `Frame::popup` with error-coloured label; Grabbing cursor |
| Tab switcher (Ctrl+Tab) | popup | `tab_switcher.rs:41` `show` | `egui::Area` + `Frame::popup` (`tab_switcher.rs:48`) | label, `Separator`, `Button::selectable` rows with tab icon |
| Start page | page in a tab | `start_page.rs:21` `show`, `start_page.rs:54` `recent_row` | tab body (`TabKey::Start`) | `heading`, two `Button`s, strong label, `Separator`, frameless `Button` per recent Project (hover = path, disabled hover = missing), frameless "×" `Button`, weak truncated `Label` |
| Focus ring | overlay | `a11y.rs:69` `focus_ring` | `egui::Area` (Order::Foreground, non-interactive) | REDA: 2 px accent `rect_stroke` around the focused widget, in keyboard mode only |
| Tooltips | tooltip | no surface of its own | stock egui tooltip via `on_hover_text` / `on_hover_ui` / `on_disabled_hover_*` | plain text almost everywhere; `on_hover_ui` with strong heading + label on bar buttons only (next row) |
| Name-and-key Tool tooltip | tooltip | `bars/draw.rs:311` `icon_button_with` (heading built at `bars/draw.rs:325`), `bars/draw.rs:672` `text_button` | stock egui tooltip | strong "Title (Key)" line; description label below, left out for a Preset; reason label when disabled |
| Native file pickers | OS dialog | `dialogs.rs:60` `FileDialogs::start_save` | `rfd::AsyncFileDialog` on a thread | not styleable from egui; listed for completeness |

### 1.2 Over the canvas (in-canvas chrome; all borderline)

| Surface | Kind | Draw function | Host | Widget kinds |
|---|---|---|---|---|
| Active Bar | overlay on canvas | `bars/draw.rs:465` `active_bar`; called from `docking.rs:615` | `egui::Area` (Order::Middle) + `Frame::window` with margin 3, radius 5 (`bars/draw.rs:504`) | REDA icon buttons (32 px / 24 px icon), REDA bar separators, group button with corner triangle, "»" More Tools stock `Button` + `Popup::menu`, Selection Filter button, snap palette button |
| Active Bar group button and menu | overlay + popup | `bars/groups.rs:58` `button` | `Popup::menu` on the corner (`bars/groups.rs:110`) | REDA icon button + REDA painted corner triangle (own `interact`); menu items via `menu_items` |
| Selection Filter button and popup | overlay + popup | `selection/filter_ui.rs:46` `button`, `selection/filter_ui.rs:8` `controls` | `Popup::menu` (`selection/filter_ui.rs:75`) | REDA painted funnel icon button; popup: `Button`, `Checkbox` list |
| Snap palette button and popup | overlay + popup | `snap/palette.rs:116` `palette_button`, `snap/palette.rs:62` `controls` | `Popup::menu`, CloseOnClickOutside (`snap/palette.rs:143`) | REDA painted icon button; popup: strong title, `Checkbox`, `Separator`, labels, `radio` rows |
| Routing Bar | overlay on canvas | `pcb/routing_bar.rs:92` `show` → `bars/draw.rs:589` `tool_bar` | `egui::Area` + `Frame::window` (`bars/draw.rs:600`) | REDA icon buttons (toggle state), REDA separators, text button (`bars/draw.rs:672`, stock `Button::frame_when_inactive(false)`) for the width value |
| Routing Bar value chooser (width / via) | popup | `pcb/routing_bar.rs:142` `chooser_ui` | `egui::Area` + `Frame::window` (`pcb/routing_bar.rs:159`) | `selectable_label` rows, `TextEdit::singleline`, label |
| Placement Bar | overlay on canvas | `pcb/placement_bar.rs:95` `show` → `tool_bar` | same as Routing Bar | same as Routing Bar |
| Tool bar note | notice | `bars/draw.rs:651` (inside `tool_bar`) | non-interactive `egui::Area` + `Frame::window` margin (6,2), radius 4 | one label |
| Layer control: swatch dropdown | overlay on canvas | `pcb/layer_control.rs:102` `swatch` (entry `pcb/layer_control.rs:88` `show`) | `egui::Area` + `Frame::window` (`pcb/layer_control.rs:115`) + `Popup::menu` | REDA layer chip; menu: chips, `Separator`, Layer-set `Button`s |
| Layer control: strip | overlay on canvas | `pcb/layer_control.rs:185` `strip` | `egui::Area`, painted `panel_fill` rect + top `hline` (`pcb/layer_control.rs:200`) | small `Button` "Sets" + `Popup::menu`, `Separator`, REDA layer chips |
| Grid readout (board canvas) | overlay on canvas | `pcb/canvas.rs:250` `overlay` (readout painted from line 285) | painter on the canvas rect | REDA: black-alpha rounded rect + white text, with an `interact` on it. Uncertain: I found this for PCB canvases only; I did not find an equivalent in `schematic_editor/` |
| Keys card ("Keys in Tool") | overlay on canvas | `keys_card.rs:101` `show` | non-interactive `egui::Area` (Foreground) + `Frame::popup` | strong title, `Grid` of monospace chord + label. Key chords are plain monospace text; there is no key-cap widget anywhere |
| In-place text edit field (schematic) | overlay on canvas | `schematic_editor/decorate.rs:100` `text_field` | `egui::Area` + `Frame::popup` (`schematic_editor/decorate.rs:111`) | `TextEdit::singleline` or `::multiline`, problem label |
| Drag / hover chip (schematic) | overlay on canvas | `schematic_editor/decorate.rs:158` `chip` | `egui::Area` (Order::Tooltip) + `Frame::popup` | one label |
| Symbol editor pin hover tip | tooltip | `symbol_editor.rs:262` `hover_tip` | `on_hover_text` on the canvas response | stock tooltip |
| Auto Wire options | floating window | `schematic_editor/auto_wire.rs:17` `options` | stock `egui::Window` (`schematic_editor/auto_wire.rs:29`) | two `Slider`s, `Button` |
| Break Wire options | floating window | `schematic_editor/break_wire.rs:152` `options` | stock `egui::Window` (`schematic_editor/break_wire.rs:163`) | `radio` rows, two `Slider`s, `Separator`, labels, `Button` |
| Pick list (ambiguous click) | floating window | `selection/picking/pick_list.rs:76` `show` | stock `egui::Window`, not resizable (`selection/picking/pick_list.rs:90`) | strong layer headings, REDA thumbnail (`pick_list.rs:293`), `selectable_label` rows, REDA lock mark |
| Show-on-board banner | overlay on canvas | `show_on_board.rs:277` `banner` | `egui::Area` + `Frame::window` (`show_on_board.rs:288`) | label coloured with `theme::tab_text`, `Button`s (Switch side, Back). Replaces the Active Bar while shown |
| Board outline hint | overlay on canvas | `pcb/canvas.rs:315` `outline_hint` | `egui::Area` + `Frame::popup` (`pcb/canvas.rs:325`) | label, `Button` |
| Canvas context menu | menu | `context_menu.rs:350` `canvas` | `response.context_menu` (`context_menu.rs:371`) | title label, `Separator`, `menu_button` submenus, items through `bars::draw::menu_items` |
| Tool previews, snap marker, violation markers, measure, handles | canvas content | `snap.rs:256`, `canvas/markers.rs:45`, `canvas/widget.rs:313`, `pcb/canvas.rs:160` | painter on the canvas rect | REDA painting in canvas colours. I count these as canvas, not chrome; listed so they are a decision, not an omission |

### 1.3 Panels (`PanelId::ALL`, `layout/model.rs:34`: nine Panels; dispatched in `docking.rs:962` `content`)

Every Panel is a tab body inside a tab group, with a 6 px margin frame. None has a header of its own.

| Panel | Draw function | Widget kinds |
|---|---|---|
| Projects | `projects_panel.rs:71` `show` | `ScrollArea`, `CollapsingHeader` sections (Project root, Schematic, PCB, Output Jobs, Libraries), tree rows = `Button::selectable` (`projects_panel.rs:545` `tree_item`), weak sheet numbers, inline rename `TextEdit`, `context_menu` on every header and row (items `projects_panel.rs:553`), drag-to-reorder with a painted `hline` drop mark (`projects_panel.rs:262`), hover text; delete confirmation is a `Dialog` (`projects_panel.rs:502`) |
| Properties: selection mode | `panels/properties.rs:78` `content` | `ScrollArea` (scroll bar always visible), count label, kind chips = stock `Button`s (`panels/properties.rs:257`), collapsible regions = `CollapsingHeader` with strong title (`panels/properties.rs:349` `region`), `Grid` label/value rows, `TextEdit` fields (`panels/properties.rs:821`), `ComboBox`, `Checkbox`, `Button`, warn-coloured labels, hover text |
| Properties: overview (nothing selected) | `panels/properties/overview.rs:25` `show` | `ScrollArea`, labels, REDA severity dot (`overview.rs:156`), REDA ok/problem status mark (`overview.rs:167`), `ComboBox` grid choices, `selectable_label` units, `Checkbox`, `small_button`, `Hyperlink`-style links, `Grid` of board figures; includes the sheet frame section (`panels/properties/sheet_frame.rs:50`: `Grid`, `Checkbox`, `ComboBox`, `radio`) |
| Properties: placing mode | `panels/properties.rs:557` `placing_section` | labels, `Checkbox`/`ComboBox`/`TextEdit` rows |
| Properties: Zone Tool options | `panels/properties/zone.rs:19` `tool_options` | `Grid`, `ComboBox`, `Checkbox`, `Button`, `Separator` |
| Properties: Arc Tool options | `panels/properties/arc_tool.rs:13` `tool_options` | labels, `radio`, `Separator` |
| Properties: another Tool running | `panels/properties.rs:97` (inline in `content`) | one weak label |
| Properties: Track / Net block | `panels/properties/track.rs:20` `show` | `Grid`, `ComboBox`, `Checkbox`, `selectable_label`, REDA net colour square (`track.rs:157`) |
| Properties: annotation (text, dimension, free pad, graphics) | `panels/properties/annotation.rs:24` `show` | `Grid`, `ComboBox`, `Checkbox`, `Button`, image button, `Separator`; free pads use the shared pad form (`pad_form/draw.rs:97` `rows`) |
| Properties: Zones section | `panels/properties/zone.rs:233` `zones_section` | as Zone Tool options |
| Properties: rules that apply | `panels/properties/rules.rs:24` `show` | `Grid`, clickable `Label`, REDA standing mark (tick / cross / dash, `rules.rs:116`), `CollapsingHeader` "Compare" with a REDA footprint drawing (`rules.rs:228`), `Button` |
| Properties: Unit rows, Library status | `panels/properties/unit.rs:67`, `panels/properties/library.rs:64` | labels, `Button`s, hover text |
| Messages | `panels/messages.rs:125` `show` (`contents` at 135) | `Frame::new` wrapper; toolbar (`messages.rs:183`): `Button`s (Run ERC / DRC), `Checkbox`, `Separator`, severity toggles = `selectable_label`, two `ComboBox`es; list: `ScrollArea`, `CollapsingHeader` groups, rows = `selectable_label` (`messages.rs:255`), row `context_menu` with `menu_button` + `radio` severity; details pane (`messages.rs:359`) hosted in a nested `egui::Panel::bottom` (`messages.rs:149`): strong wrapped `Label`, `link`s to objects, labels |
| Board navigator (Navigator) | `panels/navigator.rs:63` `show` | mode switch = `selectable_label` row (Nets / Footprints / Zones, `navigator.rs:103`), search `TextEdit`, `Separator`, column header labels (`navigator.rs:200`), `ScrollArea`, rows = truncating `Button::selectable` (`navigator.rs:171`) + `Checkbox` cells, REDA net colour swatch with `Popup::menu` colour chips (`navigator.rs:285`), REDA drag grip + drop `hline` for Zone order (`navigator.rs:485`–`535`), REDA stale-fill dot (`navigator.rs:577`), `Button`s, hover text |
| Library | `panels/library.rs:203` `show` (`draw` at 232) | filters (`library.rs:362`): `TextEdit` search, two `ComboBox`es; `ScrollArea` list, rows = `selectable_label` + weak clickable `Label` (`library.rs:430`–`436`), row `context_menu` (`panels/library/menu.rs`), drag payload to the canvas; `Separator`; buttons row: `Button`, `menu_button` New (`library.rs:308`), nested `menu_button` Generate; details (`panels/library/details.rs`): strong name, labels, unit tabs = `selectable_label`, `ComboBox` footprint, `ui.group` "where used"; REDA symbol / footprint preview (`panels/library/preview.rs:98`, `:173`, `:273`) |
| Layers | `panels/layers.rs:23` `show` | `ScrollArea`, `ComboBox` (inactive-layer mode, grid), `Checkbox` rows (visibility, object types, ratsnest), `RadioButton` per Layer (active Layer), stock colour button `color_edit_button_srgba` (`layers.rs:259`), REDA pin mark on a checkbox (`layers.rs:115`), Layer-set `Button`s, `CollapsingHeader` (ratsnest nets), `selectable_label` units, `Separator`s |
| Journal | `journal_panel.rs:21` `show` | `Button` Export, `ScrollArea::both`, `Grid` with strong column heads, monospace / plain labels, hover text (`journal_panel.rs:63` `row`) |
| Find | `panels/find.rs:306` `show` | `TextEdit` + `Button`, `Checkbox` rows, segmented choices built from `selectable_value` (syntax, scope), error `colored_label`, `Separator`, `ScrollArea` of `selectable_label` results |
| Pins | `panels/pins.rs:118` `show` (`symbol_table` at 130) | strong title, `Button` Paste, `ScrollArea::both`, `Grid` table; sortable heads = frameless strong `Button` (`pins.rs:170`); cells: `TextEdit` (`pins.rs:374`), `ComboBox`, `Checkbox`, `small_button`; paste opens the paste-map `Dialog` (`pins.rs:425`) |

### 1.4 Settings windows (non-modal; `TabKey::Settings` in a floating window, `settings/windows.rs:198` `show`)

Hosting: a settings window is a tab in a floating window (native viewport, or stock `egui::Window` when
embedded), opened at 1000×680 (`docking.rs:80`). Shell for all but Preferences: `settings/draw.rs:121`
`SettingsDialog::show`. Preferences has its own shell with the same layout.

| Surface | Draw function | Widget kinds |
|---|---|---|
| Settings window shell (Project Settings, Board Setup, Output Job, Change List) | `settings/draw.rs:121` | bottom-up layout: button row (`Button`s OK / Cancel / Apply / Run / Close, disabled hover text), error `colored_label`s, `Separator`; page tree in a `ScrollArea` (`settings/draw.rs:219` `tree`: weak group heading labels + `tree_row` = `Button::selectable`, `settings/draw.rs:290`), vertical `Separator`, page in `ScrollArea::both`; page header (`settings/draw.rs:259`): strong title + Reset `Button` |
| Preferences shell | `preferences_dialog/draw.rs:60` `show` | weak "live" label + Close `Button`, `Separator`; top bar (`draw.rs:114`): search `TextEdit`, `Checkbox` Modified only, `Button` Reset all; page tree (`draw.rs:135` `page_list`): `Button::selectable` rows with indent and weak right-text modified count; page header (`draw.rs:183`): title + Reset page / Reset branch; search results (`draw.rs:231`) |
| Preferences: schema pages (General, Window, Editing + Snap / Grid / Auto-pan / Highlight, Schematic + Wires / Names and Units, PCB + Routing / Placement / Zones / View, Cross-select, Checks, Display; list in `crates/reda-engine/src/preferences/pages.rs:30`) | `preferences_dialog/draw.rs:247` `page_content`, row `draw.rs:340` `setting_row`, editor `draw.rs:355` `editor` | strong sub-heading labels, per row: REDA reset dot (`draw.rs:318`), label, then by kind: `Checkbox`, `ComboBox`, `TextEdit`, list count label; problem labels; hover text |
| Preferences: Checks extra | `preferences_dialog/check_grid.rs:16` `show` | `ScrollArea`, `Grid`, `Checkbox` cells, labels |
| Preferences: Grid extra | `preferences_dialog/grid_tables.rs:38` `show` | `Grid`, `TextEdit`, `Checkbox`, `small_button` add / remove, `Button` |
| Preferences: Libraries | `preferences_dialog/draw.rs:433` `libraries`, `draw.rs:502` `library_severities` | labels, `Button`s, `Checkbox` (`draw.rs:890` `active_checkbox`), `ComboBox` severities, `Separator` |
| Preferences: Fab Presets | `preferences_dialog/draw.rs:813` `fab_presets`, pages `preferences_dialog/fab_preset_pages.rs:102` | `Button::selectable` list, `Button`s, `Frame::group` box, `Separator`, then the shared fab preset pages (1.5) |
| Preferences: Agents | `preferences_dialog/draw.rs:537` `agents` | labels, `Checkbox`, `Button`s, `Hyperlink` |
| Preferences: Keyboard Shortcuts | `preferences_dialog/draw.rs:577` `keyboard_shortcuts`, `draw.rs:613` `modifier_roles`, `draw.rs:646` `shortcut_row`, `draw.rs:700` `recording` | context `ComboBox`, rows: fixed-width label (`draw.rs:798`), key chips = small stock `Button` "chord ×", `small_button` Add, small Reset `Button`; recording box = `Frame::group` with labels, `ComboBox`, warn `colored_label`, `Button`s |
| Preferences: All settings | `preferences_dialog/draw.rs:289` `all_settings_table` | `Grid` table: key, editor, default, reset dot, description |
| Project Settings pages (`settings/project.rs:25`): Properties | `settings/project/properties.rs:75` | `Grid` of label + text field |
| Project Settings: ERC | `settings/project/erc.rs:364` (`check_grid` 384, `pin_matrix` 486) | `Grid`, `radio` / `selectable_label` severities, REDA pin-matrix cell (`erc.rs:542`) with `context_menu`, `Button`s, hover text |
| Project Settings: Schematic | `settings/project/schematic.rs:264` | strong headings (`schematic.rs:373`), `Checkbox`, `radio` choice groups (`schematic.rs:380`), `Grid`, `Button` |
| Project Settings: Libraries | `settings/project/libraries.rs:138` | labels, `Button`s, `TextEdit` |
| Project Settings: Fab Presets (+ one page set per Preset copy) | `settings/project/fab_presets.rs:285`, copy pages `settings/project/preset_pages.rs:120` | `Grid`, `Button`s, `Frame::group` preview box (`fab_presets.rs:165`), labels; copy pages reuse 1.5 |
| Board Setup pages (`settings/board_setup.rs:464`–`473`): rule pages Clearance, Track Width, Vias, Holes, Board Edge, Silkscreen, Mask and Paste, Zone Connections | `settings/board_setup/draw.rs:116` `rule_page` (`section_ui` 149) | `Grid` rows, `TextEdit` length fields, `ComboBox`, `Checkbox`, `radio`, `selectable_label`, "Show on board" `Button` (`draw.rs:252`) |
| Board Setup: Stackup | `settings/board_setup/draw/stackup.rs:21` `stackup_page` | `Grid` table (`stackup.rs:49`) with `TextEdit` / `ComboBox` cells, REDA cross-section drawing (`stackup.rs:142`), suggestion box = `TextEdit` + `ComboBox` (`stackup.rs:224`) |
| Board Setup: Net Classes | `settings/board_setup/draw/grid.rs:21` `net_classes_page` | toolbar `Button`s (`grid.rs:34`), spreadsheet-like `Grid` (`grid.rs:175`) with REDA row handle / drag grip (`grid.rs:90`), `TextEdit`, `ComboBox`, `Checkbox` cells, painted cell marks (`grid.rs:336`), net table (`grid.rs:377`), pattern form in `Frame::group` (`grid.rs:489`) |
| Board Setup: DRC | `settings/board_setup/draw.rs:432` `drc_page` | `Grid`, severity `ComboBox` / `selectable_label` (`draw.rs:73`) |
| Board Setup: footer actions (preset preview, save-as-preset form, rules import preview) | `settings/board_setup/draw/actions.rs:71`, `:135`, `:167` | three `Frame::group` boxes with labels, `Grid`, `Checkbox`, `TextEdit`, `Button`s |
| Output Job pages (`settings/output_job.rs:94`–`100`): General, Outputs, Gerber, Drill, BOM, CPL, Last run | `settings/output_job/draw.rs:14` `page` (`general` 62, `outputs` 163, `gerber` 217, `drill` 259, `assembly` 305, `last_run` 362) | `Grid` forms, `ComboBox`, `Checkbox`, `radio`, `Button`s, horizontal `ScrollArea` tables built from `Grid`, "from Preset" pill badge = `Frame::new` + small label (`draw.rs:40`), hover text |
| Change List | `settings/change_list/draw.rs:26` `draw` (`toolbar` 87, `group_header` 120, `row` 159, footer 280) | `Button`s, `Checkbox`es, filter `TextEdit`, REDA fold arrow (`draw.rs:288`), REDA arrow (`draw.rs:318`), REDA check mark (`draw.rs:337`), `Grid`, `selectable` rows with `context_menu`, `Separator`, `ui.group` (`settings/change_list.rs`) |

### 1.5 Fab Preset pages (shared by Preferences, Project Settings copies and Output Job "Edit Preset")

| Surface | Draw function | Widget kinds |
|---|---|---|
| Rules, Stackup, Gerber, Drill, BOM, CPL, Packaging (`settings/fab_preset.rs:26` `PresetPage`) | `settings/fab_preset/draw.rs:16` `page`; per page `rules` 108, `stackup` 170, `gerber` 197, `drill` 276, `bom` 418, `cpl` 477, `packaging` 546 | `Grid` rows (`draw.rs:29`), typed `TextEdit` (`draw.rs:37`), `ComboBox` (`draw.rs:49`), `Checkbox` (`draw.rs:72`), read-only label rows, weak notes, column editors (`draw.rs:324`) with `Button`s and `selectable_label`, hover text |

### 1.6 Modal dialogs (all through `modal::Dialog`, `modal.rs:57`, unless marked)

`modal::Dialog` = backdrop `rect_filled` (black alpha 100) on a Foreground layer + stock `egui::Window`
(title bar, movable, resizable, not collapsible). It is not `egui::Modal`.

| Dialog | Draw function | Widget kinds |
|---|---|---|
| Closing / quitting prompt | `closing_dialog.rs:11` `show` | labels, `Checkbox`, `Separator`, three `Button`s |
| Recovery prompt | `recovery_dialog.rs:23` `recovery` | labels, `ui.group` per Recovery Snapshot, `Button`s, `Separator`, hover text |
| Crash Report offer | `recovery_dialog.rs:99` `crash` | labels, `Button`s |
| New Project | `new_project.rs:80` `show` | `Grid`, `TextEdit` (`new_project.rs:165`), Browse `Button`, `radio` Fab Preset choice, error label, `Button`s |
| Command form (generic params form for any Command) | `command_dialog/draw.rs:19` `show`, row `:83`, `flag_box` `:119`, `choice_box` `:141` | `Grid`, `TextEdit`, `Checkbox`, `ComboBox` with `selectable_label`, problem labels, hover text, `Button`s |
| Notice (first Import part vendor terms) | `app.rs:834` `show_notice` | label, OK `Button` |
| Relink confirm | `app.rs:759` `show_relink_confirm` | label, two `Button`s |
| Delete confirm (Projects panel) | `projects_panel.rs:502` `confirm_delete` | label, `Button`s |
| Allow agent | `agents.rs:65` `ask` | labels, Allow / Deny `Button`s |
| Shared element (edit / copy) | `shared_element.rs:269` `show` | labels, `ScrollArea` list, three `Button`s |
| Port partner chooser | `port_partner.rs:167` `menu` | `Button` per partner, `Separator`, Cancel |
| About (tabs: About / Licences) | `help/dialogs.rs:49` `about`, `:75` `licences` | `selectable_label` tabs, labels, `Hyperlink`, `ScrollArea` with read-only multiline `TextEdit`, Close `Button` (`:128`) |
| Update check | `help/dialogs.rs:91` `update` | labels, `Hyperlink`, Close `Button`. No spinner |
| Output Job progress | `job_run/draw.rs:20` `show_progress` | labels, `ProgressBar` with percentage, Cancel `Button` |
| Output Job run report | `job_run/draw.rs:51` `show_report`, `:160` `list` | `ScrollArea`, strong / coloured labels, `Button`s |
| New Library | `library_dialogs/new_library.rs:103` | `Grid`, `TextEdit`, `radio`, `Button`s |
| Symbol wizard | `library_dialogs/wizard.rs:333` `show` (`contents` 140, `settings` 177, `grid` 231) | `ComboBox`es, `TextEdit`, pin `Grid` in `ScrollArea`, `selectable_label`, `small_button`, REDA symbol preview (`wizard.rs:387`), `Button`s. Single page despite the name |
| Paste array | `library_dialogs/paste_array.rs:275` | `Grid`, `TextEdit`, `Checkbox`, `Button`s |
| Paste map (pin table paste) | `library_dialogs/paste.rs:78` `show` (`contents` 92, `table` 163) | `ComboBox` per column, `Checkbox`, `ScrollArea::both` + `Grid`, `selectable_label`, `small_button`, `Button`s |
| Import part | `library_dialogs/import.rs:106` | `Grid`, `TextEdit` rows with Browse `Button`, labels |
| To Library (save / copy to a Library) | `library_dialogs/to_library.rs:104` | Library picker `ComboBox` (`library_dialogs.rs:98`), labels, `Button`s |
| Generate footprint (generator params form) | `library_dialogs/generate.rs:279` → `library_dialogs/generate/draw.rs:11` `show` (`sections` 96, `family_list` 192, `draw_preview` 330) | family list of `selectable_label` in `ScrollArea`, form `Grid` with `TextEdit` / `ComboBox`, REDA footprint preview with dimension callouts (`generate/callouts.rs:108`), `small_button`, `Button`s |
| Reannotate | `schematic_dialogs/reannotate.rs:345` `show` (`options` 386, `sheet_grid` 500, `preview` 541) | `Grid`, `ComboBox`, `Checkbox`, `radio`, the only `DragValue` (`reannotate.rs:488`), `TextEdit`, `ScrollArea` lists, `selectable_label`, `Separator` |
| Reannotate last (confirm) | `schematic_dialogs/reannotate.rs:718` | labels, `Button`s |
| Update from Library (two pages: setup, preview) | `schematic_dialogs/update/draw.rs:63` `show` (`setup_page` 149, `settings_grid` 209, `preview_page` 306) | `ScrollArea`, `Grid`, `Checkbox` (`tick`, `:53`), `ComboBox`, `CollapsingHeader`, `selectable_label`, many `Button`s, hover text |
| Replace Part | `schematic_dialogs/replace.rs:256` | search `TextEdit`, `ComboBox`, `ScrollArea` of `selectable_label`, labels |
| Convert cross (junction / no-connect) | `schematic_dialogs/cross.rs:106` | labels, `radio`, `TextEdit`, hover text |
| Text (schematic text params) | `schematic_dialogs/text.rs:48` | `TextEdit`, `Button`s |
| Update board from schematic | `pcb/update_dialog.rs:114` | `ScrollArea`, `Grid`, `Checkbox`, `selectable_label`, labels, `Button`s |
| Shared dialog button row (schematic dialogs) | `schematic_dialogs.rs:257` `buttons` | `Separator`, action `Button` with disabled hover text, Cancel |
| PCB text | `pcb/canvas.rs:204` `text_form` | `TextEdit`, two `Button`s |
| Board outline rectangle | `pcb/canvas.rs:365` (host call) | `Grid`, two `TextEdit`s, `Button`s |
| Choose Component (Placement) | `pcb/placement_bar.rs:173` `dialogs` (host call `:191`) | filter `TextEdit`, `ScrollArea` of `selectable_label`, `Button`s |
| Confirm locked move | `pcb/placement_bar.rs:243` `confirm_dialog` | label, two `Button`s |
| Delete dimensions too? | `pcb/delete.rs:223` `dialog` | label, three `Button`s |
| Loop gap retry | `pcb/board_shapes.rs:44` `retry_dialog` | label, two `Button`s |

### 1.7 Non-modal floating tool windows (stock `egui::Window`, no backdrop)

| Surface | Draw function | Widget kinds |
|---|---|---|
| Select Similar | `similar.rs:445` `show`, row `similar.rs:548` `rule_row` | `ScrollArea`, strong group labels, `Grid` rows with `Checkbox` + `ComboBox`, `radio` scope, `Checkbox` options, `TextEdit`, `Button`s |
| Violation details | `panels/violation_details.rs:29` `details_window` (`contents` 47, `waiver` 100) | `Grid`, labels, `Hyperlink` / `link`, `Separator`, `Button`s, `TextEdit` |
| Waive violation | `panels/violation_details.rs:201` `waive_window` | label, multiline `TextEdit`, `Button`s |
| Auto Wire options, Break Wire options, Pick list | see 1.2 | |

### 1.8 Element editors' own chrome (inside a Document tab; `element_editor.rs:222` `panels`)

| Surface | Draw function | Host | Widget kinds |
|---|---|---|---|
| Footprint editor inspector | `footprint_editor/inspector.rs:52` `show` (`footprint_section` 105, `tool_settings_ui` 216, `selection` 327, `text_properties` 400) | nested `egui::Panel::right`, fixed width (`element_editor.rs:226`) | `ScrollArea`, `Grid`s, text fields (`inspector.rs:562`), `ComboBox` (`inspector.rs:540`), `Checkbox`, `selectable_label`, `Button`s, `small_button`, `Separator` |
| Pad form (shared by the inspector and Properties) | `pad_form/draw.rs:97` `rows`; row helpers `:456`–`:493` | caller's `Grid` | `TextEdit` length / angle fields, `ComboBox`, `Checkbox`, `selectable_label` |
| Symbol editor Unit tabs and text entry | `symbol_editor/view.rs:15` `panels` (`unit_tabs` 23, `text_entry` 57) | nested `egui::Panel::top` (`symbol_editor/view.rs:17`) | `selectable_label` tabs, `Button`, `Separator`, `TextEdit` |
| Part editor (fills its tab; no canvas) | `part_editor/view.rs:20` `show` (`header` 73, `properties` 118, `symbol` 224, `footprints` 304, `pin_map` 470, `fields` 611) | tab body, `ScrollArea` | collapsible regions = `CollapsingHeader` (`view.rs:51`), `Grid`s, text fields, `ComboBox` pickers, `Checkbox`, many `Button`s, `menu_button` Generate footprint, `ui.group`, `Hyperlink`, REDA red outline on bad pin-map cells (`view.rs:551`), Library previews |
| Shared text field for the editors | `element_editor.rs:321` `text_field`, `:332` `text_field_with` | | `TextEdit::singleline` committing on focus loss |

## 2. Widget kinds

### 2.1 Stock egui widgets (painted by egui from `theme::visuals`)

| Widget kind | Used by |
|---|---|
| `Button` (text) | nearly every surface; dialogs' button rows, panels' toolbars, settings footers |
| `Button` with image + text | menu items (`bars/draw.rs:193`), Command Palette rows, tab switcher rows |
| `Button::frame(false)` (flat text button) | status bar segments, Start page recent rows and "×", Pins column heads |
| `Button::frame_when_inactive(false)` | Active Bar "»", tool bar text button |
| `small_button` / `Button::small()` | status bar Dismiss / Refill, shortcut chips and Add, grid tables, agents Forget, Layer strip Sets, wizard, overview |
| `Button::selectable` / `selectable_label` / `selectable_value` (list row, tree row, tab, segmented choice) | Projects tree, settings and Preferences page trees, Navigator rows and mode switch, Library list and unit tabs, Messages rows and severity toggles, Find results and syntax / scope, pick list, tab switcher, About tabs, Symbol Unit tabs, grid menu, routing chooser, Layers units, many dialogs' lists |
| `Checkbox` | Layers, Find, Messages toolbar, Navigator cells, Pins, Properties, Preferences rows, settings pages, Selection Filter, snap palette, grid menu, agents menu, closing prompt, dialogs |
| `RadioButton` / `ui.radio` | Layers (active Layer), snap palette, Break Wire, Messages severity menu, New Project, New Library, Reannotate, Convert cross, Select Similar, Arc Tool, sheet frame, ERC, Schematic page, Board Setup, Output Job |
| `ComboBox` | Properties, Layers, Library filters and details, Messages toolbar, Pins cells, Preferences editors and shortcut context, Board Setup, fab preset pages, Output Job, command form, wizard, paste map, To Library, Reannotate, Update, Replace, Select Similar, inspector, pad form, Part editor |
| `TextEdit` single line | Command Palette, searches (Preferences, Library, Navigator, Find, Change List, Choose Component, Replace), every form field, in-place canvas edit, routing chooser, Projects rename |
| `TextEdit` multiline | in-place canvas edit, About licences, waive reason, some forms |
| `DragValue` | Reannotate start index only (`schematic_dialogs/reannotate.rs:488`) |
| `Slider` | Auto Wire options, Break Wire options only |
| `CollapsingHeader` | Projects sections, Properties regions, Part editor regions, Messages groups, Layers ratsnest nets, rules Compare, Update dialog |
| `ScrollArea` | every Panel, both settings shells, Command Palette, most list dialogs |
| `Grid` | all forms and tables (Journal, Pins, All settings, Net Classes, Stackup, Output Job tables, keys card). There is no `TableBuilder`: every table is a `Grid`, sometimes with hand-set column widths |
| `Separator` | menus, status bar, panels, dialogs, settings shells |
| `Hyperlink` / `ui.link` | About, Update check, Preferences (Agents, Libraries), Messages details, Violation details, Properties overview, Part editor |
| `ProgressBar` | Output Job progress only |
| `Spinner` | not used |
| `menu_button` / `MenuBar` / `Popup::menu` / `context_menu` | main menu bar, quick strip submenus, Active Bar More Tools and groups, status bar (grid, Layer, Panels, agents), Library New / Generate, Part editor Generate, tab menu, canvas context menu, Projects / Messages / Library / Change List / ERC matrix row menus, Navigator colour menu, Layer control |
| Colour button (`color_edit_button_srgba`) | Layers panel only (`panels/layers.rs:259`) |
| `Image` (SVG icons via `icons::image`, `icons.rs:235`; silhouettes `icons.rs:209`) | menu items, bar buttons, tabs, Dock icon strip, Command Palette rows, tab switcher, one image button in Properties annotation |
| Labels: plain, `weak`, `strong`, `heading`, `small`, `monospace`, `colored_label` (warn / error) | everywhere; `heading` only on the Start page; section heads are `strong` labels |
| `ui.group` / `Frame::group` (boxed section) | Recovery prompt, Library where-used, Part editor, Change List, Preferences (recording box, Fab Presets), Project Fab Presets preview, Board Setup actions and pattern form |
| `Frame::popup` (popup surface) | Command Palette, tab switcher, Dock flyout, keys card, in-place edit, chip, outline hint, drop refusal |
| `Frame::window` (floating bar surface) | Active Bar, Routing / Placement Bar, tool bar note, Layer swatch, routing chooser, show-on-board banner |
| `egui::Window` | `modal::Dialog`, Select Similar, Violation details, Waive, Auto Wire, Break Wire, pick list, embedded floating windows |
| Tooltip (`on_hover_text` / `on_hover_ui` / `on_disabled_hover_text`) | 80 call sites; `on_hover_ui` only in `bars/draw.rs` |

### 2.2 REDA-drawn widgets

| Widget | Defined at | Used by | States drawn |
|---|---|---|---|
| Icon button (square) | `chrome.rs:29` `icon_button` | quick strip, gear, Active Bar, Routing / Placement Bar, Dock icon strip, group face | 5: rest (transparent), hover (`hovered.weak_bg_fill`), pressed (`active.weak_bg_fill`), selected (`selection.bg_fill`), disabled (icon tinted to 47 % alpha). No focus state of its own; the global focus ring covers it |
| Bar separator | `chrome.rs:66` `bar_separator` | quick strip, Active Bar, tool bars | 1 |
| Tab | `docking.rs:723` `tab` | every tab strip | 3: inactive, hovered (white alpha 20 fill), active (`theme::ACTIVE_TAB_FILL` + 2 px white underline); text colour from `theme::tab_text(active)`. Colours are literals for a blue bar, not from visuals |
| Tab close "×" | `docking.rs:767` (`interact`), painted `docking.rs:817` | tabs; shown on active or pointed tab | 2: rest, hover (text colour only) |
| Tab "out of sync" badge (pill) | `docking.rs:807` (colours at `docking.rs:736`) | PCB tab | 1; hard-coded dark-on-amber |
| Tab strip bar and group body | `docking.rs:670` `group_at` | every tab group | 2 bar colours: group holding the active Document, others (`theme::tab_bar`) |
| Gap between groups / Docks | `docking.rs:206`, `docking.rs:651` | document area, floating windows | 1; not interactive |
| Dock expand chevron | `docking.rs:475` + `docking.rs:1201` `chevron` | collapsed Dock strip | 2: rest, hover (stroke colour) |
| Drop-target button | `docking/drag.rs:459` `draw_button` | tab drag overlay | 2: rest, targeted (fill and mark swap) |
| Drop preview rect and dragged-tab chip | `docking/drag.rs:383` `overlay` | tab drag overlay | 2: allowed (accent), refused (error colour) |
| Group corner triangle | `bars/groups.rs:88` (in `button`, `:58`) | grouped Active Bar | 2: rest (weak), hover (strong) |
| Selection Filter button (painted funnel) | `selection/filter_ui.rs:46` `button` | Active Bar | 2: rest, hover (stroke colour). No pressed / selected / disabled, unlike `icon_button` beside it |
| Snap palette button (painted icon) | `snap/palette.rs:116` `palette_button`, icon `:159` | Active Bar | 2: rest, hover. Same gap as above |
| Layer chip (swatch + name) | `pcb/layer_control.rs:245` `chip` | Layer swatch, Layer strip, Layer menus, status bar | 4 via `interact_selectable`: rest (no frame), hover, pressed, selected |
| Layer strip background | `pcb/layer_control.rs:204` | Layer strip | 1 |
| Net colour swatch and colour chips | `panels/navigator.rs:285` `swatch` | Navigator Nets mode | swatch 2 (colour set / none = outlined square with a diagonal); chips 2 (rest, current outlined). Hover is not drawn. Uncertain about pressed: I read only lines 285–320 |
| Net colour square (display only) | `panels/properties/track.rs:157` | Properties Net block | 1 |
| Drag grip + drop line | `panels/navigator.rs:485`, `:566` `grip_lines`, `:533` | Navigator Zones order | 1 + drop indicator |
| Row drop line | `projects_panel.rs:262` | Projects sheet reorder | 1 |
| Row handle / grip (spreadsheet) | `settings/board_setup/draw/grid.rs:90` `handle` | Net Classes grid | 2: rest, selected (`selection.bg_fill`) |
| Stale-fill dot | `panels/navigator.rs:577` `stale_marker` | Navigator Zones | 1 (shown or not) |
| Severity dot | `panels/properties/overview.rs:156` `severity_dot` | Properties violations, overview | 1 per severity colour |
| Status mark (tick / problem) | `panels/properties/overview.rs:167` `status` | Properties overview readiness | 2: ok, not ok |
| Standing mark (tick / cross / dash) | `panels/properties/rules.rs:116` `mark` | Properties rules | 3 by standing |
| Check mark, arrow, fold arrow | `settings/change_list/draw.rs:337`, `:318`, `:288` | Change List | fold arrow follows `style.interact` (rest / hover / pressed) and 2 shapes (open / closed); others 1 |
| Reset dot | `preferences_dialog/draw.rs:318` `reset_dot` | Preferences rows, All settings | 2: modified (accent dot, clickable), unmodified (empty space). No hover state |
| Pin-matrix cell | `settings/project/erc.rs:542` `matrix_cell` | Project Settings ERC | per severity fill; focus / hover stroke and a "changed" dot. I did not enumerate the exact states |
| Pin mark on a checkbox | `panels/layers.rs:115` | Layers | 1 |
| "From Preset" pill | `settings/output_job/draw.rs:40` | Output Job pages | 1 (`Frame::new` with fill, stroke, radius 4) |
| Grid readout | `pcb/canvas.rs:285`–`293` | board canvas | 1; hard-coded black alpha 140 / white |
| Preview canvases (symbol, footprint, wizard, stackup cross-section, compare, pick-list thumbnail, generator callouts) | `panels/library/preview.rs:98`, `library_dialogs/wizard.rs:387`, `settings/board_setup/draw/stackup.rs:142`, `panels/properties/rules.rs:228`, `selection/picking/pick_list.rs:293`, `library_dialogs/generate/callouts.rs:108` | Library panel, Part editor, dialogs, Board Setup | not interactive; background `extreme_bg_color`, radius 4 or 2 |
| Bad-cell outline | `part_editor/view.rs:551` | Part editor pin map | 1 |
| Focus ring | `a11y.rs:69` | whole app | 1 |
| Modal backdrop | `modal.rs:64` | all modal dialogs | 1 |

Asked about but not present as distinct widgets: key-chord caps (chords are monospace or weak text, or
`shortcut_text` in menus), segmented controls (rows of `selectable_value`), section headers (strong labels
or `CollapsingHeader`), toasts (status bar text only), spinner.

## 3. Shared building blocks versus one-offs

Restyle once:

- `theme.rs:160` `visuals` and `theme.rs:225` `style`: every stock widget (buttons, checkboxes, combos,
  text fields, menus, popups, windows, scroll bars, separators, tooltips). This covers most of the app,
  because most surfaces are stock widgets in stock layouts.
- `modal.rs:57` `Dialog::show`: all modal dialogs (35 `Dialog::new` call sites) (frame, backdrop, sizing). No dialog bypasses it.
- `settings/draw.rs:121` `SettingsDialog::show` with `tree_row` (`settings/draw.rs:290`): Project Settings,
  Board Setup, Output Job, Change List shells.
- `settings/fab_preset/draw.rs:16` `page`: fab preset pages in all three places they show.
- `chrome.rs:29` `icon_button` and `chrome.rs:66` `bar_separator`: quick strip, gear, Active Bar,
  Routing / Placement Bar, Dock icon strip.
- `bars/draw.rs:173` `menu_items`: main menus, bar submenus, More Tools, group menus, canvas context menu.
- `bars/draw.rs:311` `icon_button_with`: bar button tooltips and accessibility names.
- `bars/draw.rs:589` `tool_bar`: Routing Bar and Placement Bar.
- `docking.rs:670` `group_at` and `docking.rs:723` `tab`: every tab strip, in the centre, Docks and
  floating windows.
- `pcb/layer_control.rs:245` `chip`: every place a Layer is offered.
- `pad_form/draw.rs:97` `rows`: pad editing in the inspector and Properties.
- `panels/library/preview.rs`: symbol / footprint previews in the Library panel, Part editor and dialogs.
- `a11y.rs:69` `focus_ring`: focus indication for all widgets, stock or REDA-drawn.

Rolled separately (same idea, several implementations):

- Settings shell: Preferences (`preferences_dialog/draw.rs:60`) repeats the layout of
  `SettingsDialog::show` with its own page list (`draw.rs:135`), which builds the same selectable row inline
  instead of calling `tree_row`.
- Tree / list rows: `projects_panel.rs:545` `tree_item`, `settings/draw.rs:290` `tree_row`, Preferences
  inline row, `panels/navigator.rs:171` `name`, plus bare `selectable_label` in Library, Messages, Find,
  pick list and dialogs. All are stock selectable buttons, so visuals follow the theme, but height, width
  and truncation are set in each place.
- Collapsible section: `panels/properties.rs:349` `region` and `part_editor/view.rs:51` `region` are two
  copies; Projects, Messages, Layers and Update call `CollapsingHeader` directly.
- Menu item with a refusal reason: `docking.rs:939`, `projects_panel.rs:553`, `panels/messages.rs:313`,
  beside `menu_items`.
- Form field helpers (label + `TextEdit` / `ComboBox` row): `element_editor.rs:321`,
  `panels/properties.rs:821`, `footprint_editor/inspector.rs:540` / `:562`,
  `panels/properties/annotation.rs:126`, `pad_form/draw.rs:456`–`493`, `settings/fab_preset/draw.rs:37` /
  `:49`, `settings/project/schematic.rs:380`, `command_dialog/draw.rs:83` / `:141`, `new_project.rs:165`,
  `library_dialogs/import.rs:68` / `:89`, `library_dialogs/new_library.rs:86`, `library_dialogs/wizard.rs:380`.
- Dialog button rows: only the schematic dialogs share one (`schematic_dialogs.rs:257`); `help/dialogs.rs:128`
  has its own Close; every other dialog lays out its buttons inline, with differing order and spacing.
- Floating surfaces over the canvas: seven `Area` + `Frame::window` and eight `Area` + `Frame::popup` call
  sites, each setting its own margin and radius (Active Bar and tool bar share constants; the rest do not).
- Painted small icon buttons: Selection Filter (`selection/filter_ui.rs:46`) and snap palette
  (`snap/palette.rs:116`) paint their own glyph and hover instead of using `chrome::icon_button`, so they
  lack pressed / selected / disabled states. The Dock chevron and group corner triangle are also their own.
- Status marks: tick / cross / dot are drawn separately in `overview.rs:167`, `rules.rs:116`,
  `change_list/draw.rs:337`, `navigator.rs:577`, `overview.rs:156`.
- Colour swatches: `navigator.rs:285`, `track.rs:157`, the square inside `layer_control.rs:245`, and the
  stock colour button in `layers.rs:259`.
- Non-modal tool windows (Select Similar, Violation details, Waive, Auto Wire, Break Wire, pick list) each
  configure a stock `egui::Window` directly; there is no shared non-modal frame.
- Tables: every table is a hand-built `Grid` (Journal, Pins, Net Classes, Stackup, All settings, Output Job
  files, paste map, Change List), each with its own header styling.

## 4. Not verified / uncertain

- Widget kinds per surface are from static reading; a widget behind a rarely taken branch may be missing
  from a row. 
- Grid readout: found only for board canvases (`pcb/canvas.rs:285`). I did not find a schematic equivalent
  and did not rule one out in `schematic_editor.rs` (1,245 lines, not read in full).
- Navigator swatch and ERC matrix cell: state counts are from partial reads.
- `panels/properties.rs:136`–`256` (the selection branch) was read as an outline, not line by line; the
  per-mode rows list the section functions it calls.
- `pcb/`, `schematic_editor/`, `canvas/` and `selection/` were only searched for hosts and painter calls;
  canvas-coloured painting there is treated as canvas, not chrome.
- Embedded versus native floating windows: which path runs on a user's machine depends on
  `ctx.embed_viewports()`; I did not check which platforms take which.
