# v0.1 design language

How REDA's application chrome looks: colour, type, spacing, radius, elevation, motion and the recipe of each component, in light and dark and in two Densities. Behaviour and structure (what a tab does, what the Title bar holds, which Preferences exist) are in [`app.md`](app.md). Crate: `reda-ui`, modules `theme` and `widgets`.

This file describes the look `reda-ui` draws. It was written ahead of the build, and the build's slices brought the two in step; the images under [Images](#images) are drawn by the tests.

Sources, all tickets of the [theme baseline map](https://github.com/openreda/reda/issues/266): [Which visual direction does the chrome take?](https://github.com/openreda/reda/issues/272), [Where does colour carry meaning in the chrome?](https://github.com/openreda/reda/issues/273), [What does Density change, and how is it chosen?](https://github.com/openreda/reda/issues/271), [How are the chrome's text size and typeface chosen?](https://github.com/openreda/reda/issues/277), [How are theme tokens structured and enforced in reda-ui?](https://github.com/openreda/reda/issues/275) and [ADR-0015](../../adr/0015-chrome-styling-goes-through-theme-tokens-and-reda-widgets.md), [What does each component look like in the chosen direction?](https://github.com/openreda/reda/issues/276), [Which transitions does the chrome animate, and with which timing?](https://github.com/openreda/reda/issues/282), [Do icons change under the new theme, and do the hand-painted glyphs become icons?](https://github.com/openreda/reda/issues/283), [Does REDA draw its own title bar, and what goes in it?](https://github.com/openreda/reda/issues/274) and [ADR-0014](../../adr/0014-reda-draws-its-own-title-bar.md), [Where does the design-language spec live, and what does it replace?](https://github.com/openreda/reda/issues/285). Where two of them disagree, the later resolution wins, and this file follows it.

**Who is the authority.** This file is the authority for intent: which token does which job, and what each component is made of. The `Tokens` value in `reda-ui` is the authority for values. The tables under [Values](#values) are generated from it by `cargo xtask tokens` ([Keeping the values in step](#keeping-the-values-in-step)).

## Scope

- **Chrome only.** Everything REDA draws around a Document's canvas: the Title bar, tabs, Docks, Panels, menus, dialogs, settings windows, the status bar, and the bars that float over a canvas.
- **Not the canvas.** Layer colours, the canvas background, the grid and markers are a separate effort. The canvas takes the few chrome colours it needs (the selection box, the touch-selection green) through `theme::canvas_theme` and defines none of its own.
- **Four variants.** Light and dark, each in Comfortable and Compact. The theme follows the OS setting by default.
- **Not user-selectable:** the accent. REDA picks one, so that contrast can be checked.

## The look

The direction is called **Paper**:

- **Warm neutral surfaces** a few percent apart: a base colour for the window, slightly lighter cards on it, and lighter still for what floats.
- **Tab groups are cards.** Each tab group and Dock is a card with 12 px corners, a 1 px rim and a 1 px soft shadow, set 6 px from its neighbours and 6 px in from the window edge, on the base colour. The Title bar and status bar sit directly on the base with no separator lines.
- **Shadows only on what floats:** menus, popups, tooltips, floating bars, dialogs and windows. Cards have only the 1 px shadow.
- **One blue accent, used sparingly,** and three semantic colours. Everything else is neutral. See [Colour](#colour).
- **Inter at 13 px,** one size throughout the chrome, with 30 px controls in Comfortable and 26 px in Compact.
- **Coloured icons** as before, in their role colours.

## Tokens

Settled in [ADR-0015](../../adr/0015-chrome-styling-goes-through-theme-tokens-and-reda-widgets.md).

- **One source.** A single `Tokens` value holds every colour, space, radius, text style, shadow and duration. It lives in the egui `Context` and is rebuilt when the theme, Density, text size or typeface changes. Code reads it through `ui.tokens()` as plain fields, such as `t.color.surface.card`, `t.space.gap` and `t.radius.card`.
- **Three layers:**
  - a **palette** of raw hexes, private to `theme`;
  - **tokens**, named by job and never by value (there is no `grey_300` and no `space_8`);
  - **recipes** in `widgets`: each component's choice of tokens per state. A recipe holds no numbers of its own, apart from named constants for the dimensions that no setting changes ([Fixed values](#fixed-values)).
- **egui's `Style` is generated** from `Tokens` by `theme::style(tokens)`, so stock and third-party widgets (the file dialog) get sensible defaults. Outside `theme` and `widgets`, code does not read `ui.visuals()` or `ui.spacing()`.
- **Components are REDA widgets,** thin builders on stable egui such as `widgets::button(..).primary()` and `widgets::text_field(..)`. egui's `experimental` widget themes stay off.
- **Each family follows one setting,** and no token depends on two:

  | Family | Follows | Holds |
  |---|---|---|
  | `color.surface` | light/dark | base, card, floating, well, scrim |
  | `color.text` | light/dark | primary, muted |
  | `color.stroke` | light/dark | hairline, strong, focus_ring |
  | `color.fill` | light/dark | control, hover, press, selected_unfocused |
  | `color.accent`, `color.error`, `color.warning`, `color.success` | light/dark | text, subtle, solid, on_solid |
  | `color.touch`, `color.close_hover`, `color.icon.*` | light/dark | touch selection; the close button's hover; the six icon role colours |
  | `elevation` | light/dark | card, popup, window |
  | `space` | Density | 13 paddings and gaps, two hand-picked values each |
  | `type` | text size and typeface Preferences | small, body, body_strong, title, heading, mono, numeric, numeric_small |
  | `size` | text size | scale, dock, field; every other field, dialog and Dock column width is a constant at 13 px read through `size.of(..)` |
  | `radius`, stroke widths, icon sizes, `opacity.disabled` | fixed | |
  | `motion` | fixed | fast, base |

- **Heights are not tokens.** They are computed from line height plus padding ([Space and Density](#space-and-density)).
- **Rust field names** follow the names here, with these differences: the `type` family is `ty` (`type` is a keyword), stroke widths are `stroke.hairline`, `stroke.accent_line` and `stroke.focus_ring` (the colours are `color.stroke.*`), icon sizes are `icon.small` and `icon.bar`, and the fixed dimensions of single components (the rows without a token name under [Fixed values](#fixed-values)) are not tokens: each is a named constant beside its recipe in `widgets`. The `size` family is added by the first slice that needs a width.

## Colour

Values: [Colour values](#colour-values).

### Surfaces, text and strokes

| Token | Job |
|---|---|
| `surface.base` | The window: behind cards, and under the Title bar and status bar. Also a settings window's sidebar and a collapsed Dock's icon strip. |
| `surface.card` | Tab groups, Docks and the front tab. |
| `surface.floating` | Menus, popups, tooltips, floating bars, dialogs and the Command Palette. |
| `surface.well` | Text fields and other wells, and the unchecked check box and radio. |
| `surface.scrim` | Laid over the window behind a modal. |
| `text.primary` | Ordinary text. |
| `text.muted` | Secondary text: property names, shortcuts, captions, back tabs, table headers, units, the status bar, waived rows. |
| `stroke.hairline` | Card rims, separators, the rule under a table header. |
| `stroke.strong` | Field, check box and radio outlines. |
| `stroke.focus_ring` | The keyboard focus ring. |

There is no disabled text colour. Disabled is one opacity, `opacity.disabled` (45 %), applied once to the whole widget, icon included.

### State fills

- **Four neutral fills,** each the primary text colour at a low opacity, laid over whatever is underneath:
  - `fill.control`: a filled button or combo box at rest.
  - `fill.hover`: hover on anything flat (rows, menu items, flat buttons, tabs), and added on top of a filled control when hovered.
  - `fill.press`: replaces the hover fill while pressed.
  - `fill.selected_unfocused`: the selected row where the selection is not live.
- **One rule covers every control.** Because the fills are translucent, the same hover and press fills go over a neutral control, an accent solid and an error solid. One overlay function in `theme` does this, and it is the only colour blending allowed. No state needed a token of its own.

### The accent

The blue accent has seven jobs and no others:

1. **Selection:** selected rows, selected text, the enclosing selection box.
2. **The keyboard focus ring.**
3. **On state:** check boxes, radio buttons, switches, toggles, the active Tool's button.
4. **The one primary button** in a dialog.
5. **Links.**
6. **Drag-and-drop targets:** tab drop zones, a splitter being dragged.
7. **Progress:** bars and busy indicators. A job run report's final state turns to success or error.

### Semantic colours

- **Three only:** error (red), warning (amber) and success (green). Informational messages are neutral, with no fourth hue.
- **Where they show:**
  - severity marks, and the Errors and Warnings counts in Messages, tinted only when the count is above zero;
  - invalid-field outlines and their message text;
  - the out-of-sync badge on a tab (warning, solid);
  - OK ticks (success);
  - the confirm button of a destructive dialog (error, solid).
- **Destructive menu items** such as Delete stay neutral.
- **A waived Violation and a Stale Waiver** draw in muted text with no severity colour.
- **Colour is never the only signal.** Each semantic colour comes with a shape or a word: a cross, a triangle, a tick, the count's label.

### Three strengths

The accent and each semantic colour are a **tone** with three strengths:

| Strength | Used for |
|---|---|
| `text` | text, Glyphs and lines |
| `subtle` | a translucent tint behind a row, a cell or a count |
| `solid` | a fill with its own `on_solid` colour: badges, the primary and destructive buttons, checked boxes |

The warning solid is a real amber with dark text on it. The other solids carry white.

### Selection and window focus

- **A selected row** is the accent subtle tint with ordinary text. It is not accent-coloured text and not a solid fill with white text.
- **Only one place shows a live blue selection.** In a Panel that does not hold keyboard focus, the selected row is `fill.selected_unfocused`.
  - The Panel that holds it, for this, is the one in use: the Panel last pressed in or moved into with the keyboard, and otherwise the tab the layout focuses. A press on a row never leaves that row looking unfocused, whether or not its Panel takes the layout's focus.
- **When REDA's window is in the background,** the active tab's accent line and the live selection tint go neutral. In the Title bar, text and window-control Glyphs go muted. Nothing else dims, and no surface changes.

### Other colour sources

- **Touch-selection green** is its own token, `color.touch`, separate from success even where the two are close.
- **Icons** keep their six role colours at full strength everywhere, including back tabs, and go grey only when disabled ([Icons and Glyphs](#icons-and-glyphs)).
- **The kind of a Document** is carried by its tab icon alone (green for a schematic, orange for a PCB). There are no tints per area or per Document kind: no coloured Panel headers, and no wash on tab groups or the status bar.
- **Data colours** (Layer, Net, Net Class, stackup) stay outside the theme. They show in one swatch shape with a neutral rim, and never borrow the accent or a semantic colour.

### What stays neutral

Surfaces, hairlines, hover and press fills, plain buttons, menu highlights, section headers, tooltips, scrollbars, the Title bar and the status bar.

## Type

Values: [Type values](#type-values). The Preferences are specified in [app.md](app.md#preferences).

- **Faces.** The bundled UI face is **Inter**, and the bundled monospace face is **JetBrains Mono**. The user can replace either with an installed system font (`general.ui_font`, `general.mono_font`).
- **Weights** are regular, medium and semibold. egui has no weight on a text style, so each weight is installed as its own named font family.
- **A text style is a whole token:** family, weight, size and line height together. Widgets never build a `FontId` or set a size or weight. Bold running text is `type.body_strong`.

  | Style | Job |
  |---|---|
  | `small` | Captions, table headers, the status bar, key chips, badges. |
  | `body` | Everything else. Menus are the same size as the rest. |
  | `body_strong` | Emphasis in running text, and the active Document's tab. Medium. |
  | `title` | Dialog titles and Panel section headers. Semibold at the body size. |
  | `heading` | Page headings (the Start page, Preferences pages). Semibold. |
  | `mono` | The Journal, file names and patterns, command and key text. |
  | `numeric`, `numeric_small` | Numbers that must line up: tabular digits. |

- **Text size** (`general.text_size`) is the body size in whole pixels, 11 to 16, default 13. Every other style moves by the same pixel offset, and no style goes below 10 px. Icons, strokes and radii do not move. It is independent of UI scale, and the two multiply.
- **Line height is a token,** fixed per text size and independent of the face: `2 * floor(0.75 * size + 0.5)`, about 1.5 em on a 2 px grid. Glyphs are centred in it from the face's ascent and descent, so a control is the same height with any face. A very tall face may sit tight or clip slightly.
  - Every REDA widget builds its text from a text-style token (`TextFormat::line_height`). A plain string handed to a stock widget takes the face's own row height, which is wrong.
  - A stock `TextEdit` takes the line height through its layouter and also needs `interact_size.y` set to it.
- **Tabular digits.** egui cannot switch on OpenType `tnum`, so the bundled Inter is shipped a second time with its tabular digits baked into the character map (done once in-house, with the result checked in). The numeric styles use it. Numeric recipes: number fields, coordinates and deltas in the status bar, number columns, counts, and the Command Palette's result count.
- **When the chosen face does not fit:**
  - **Weights:** each resolves to the weight axis if the font is variable, otherwise to the family's nearest installed static weight, otherwise to regular. Bold is never faked. No recipe relies on weight alone to carry meaning, so a regular-only face stays usable.
  - **Digits:** numeric styles use the chosen face if its digits measure equal width, otherwise the bundled Inter.
  - **Missing glyphs:** the chosen face, then the bundled face, then egui's built-in fonts. Scripts none of them cover show as boxes.
  - **Widths** follow text size only and are not re-measured per face, so a wide face truncates with an ellipsis sooner.

## Space and Density

Values: [Space values](#space-values).

- **Density changes room and nothing else:** the padding inside controls and containers and the gaps between items, and through them the height of controls, rows, tabs and the Title bar.
- **Fixed across Densities:** text sizes, icon sizes, radii, stroke widths, colours, shadows and motion. Where a radius would exceed half a control's height it clamps; that is a rendering rule, not a second radius token.
- **Two groups of size tokens.** The `space` group has a Comfortable and a Compact value per token. Everything else has one value. A widget reads no raw number from either.
- **Compact is specified, not computed.** Each `space` token carries two hand-picked values on a 2 px grid, with Compact usually one step down. There is no multiplier.
- **Heights are derived:** line height plus twice the vertical padding.

  | Height | Formula | At 13 px text (Comfortable / Compact) |
  |---|---|---|
  | Control (button, field, combo box) | body line + 2 × `control_y` | 30 / 26 |
  | Row (list, tree, table, menu) | body line + 2 × `row_y` | 28 / 24 |
  | Tab | body line + 2 × `tab_y` | 34 / 30 |
  | Title bar | control height + 2 × `bar_y` | 40 / 36 |
  | Status bar | small line + 8 | 24 at both |
  | A floating window's bar | body line + 12 | 32 at both |

- **Surfaces that ignore Density:** the status bar, tooltips, a floating window's bar and anything drawn on the canvas. They still follow the text size.
- **Widths never follow Density.** Field, dialog and default Dock column widths are `size` tokens, set relative to the text size. Sizes a Layout stores (splits, dragged Dock sizes) never change on their own.
- **Whole chrome at once.** Density applies to every window, with no override per Panel or Dock. A component that is always dense gets that from its own recipe in both Densities.
- **Density is not UI scale.** UI scale magnifies everything in proportion. Density works at any UI scale and never touches the zoom factor.

## Shape and elevation

Values: [Fixed values](#fixed-values) and [Elevation values](#elevation-values).

- **Radii,** from small to large: mark (check boxes, swatches, key chips), row (rows and menu items), control, card, popup, window. The row radius is the popup radius minus the popup's padding, so the corners nest.
- **Stroke widths:** a 1 px hairline, a 2 px accent line, a 2 px focus ring.
- **Three shadows:**
  - `elevation.card`: the 1 px soft shadow under a card.
  - `elevation.popup`: menus, popups, tooltips, floating bars and Dock flyouts.
  - `elevation.window`: dialogs, the Command Palette and windows.
- **Whole points only.** egui takes radii, margins and shadow offsets as whole points.
- **Cards around a canvas** are rounded by painting corner masks over it. The canvas itself cannot be clipped round.

## Motion

Values: [Motion values](#motion-values).

The profile is called **Crisp**: transitions are short and eased, things arrive with a fade and a few pixels of travel and leave faster than they came, and nothing delays a click or a key.

- **Two durations:** `motion.fast` (90 ms) and `motion.base` (140 ms).
- **One easing:** ease-out (cubic). Hover fills are linear.
- **Immediate:** hover arriving, a press arriving, and selection.
- **Tooltips** wait 500 ms. When a tooltip was showing within the last 400 ms, the next one shows without the wait and without a fade.
- **Never animated:**
  - the keyboard focus ring and the caret;
  - selection;
  - what a tab holds (Documents and Panels swap at once);
  - anything being dragged;
  - layout while the window is resized;
  - the canvas, whose zoom, flip and Layer-switch animation is its own Preference.
- **On the canvas side of the chrome,** the Active Bar never moves. Only a Tool's bar appearing and an on state animate.
- **The Animate Preference** (`general.animate`) turns every transition in the table off, so each change happens at once. The tooltip wait stays, because it is a wait and not a movement. How it starts from the OS reduced-motion setting is in [app.md](app.md#preferences).
- **For the build:**
  - Fade, travel and scale need only `Ui::with_visual_transform` and `Ui::multiply_opacity`: paint once at the final place, then move the shapes.
  - egui's `animate_bool` cannot give different in and out times, so REDA keeps a small engine of its own (start value, target, start time and easing per id).
  - egui 0.37's popup and menu containers have no fade setting, so menus with these timings need REDA's own popup widget.

## Icons and Glyphs

Behaviour, keys and art rules are in [app.md](app.md#icons).

- **An Icon** stands for a Command, Preset, Tool, Panel or kind of Document, and carries role colours that never change with state.
- **A Glyph** is part of a control's own drawing. It is one colour and takes the control's text colour, so it follows hover, muted, disabled and white-on-accent states.
- **Icon sizes,** at every text size and in both Densities (only UI scale changes them):

  | Size | Where |
  |---|---|
  | 20 px | the Title bar's quick strip, Tool bars, the Active Bar, collapsed Dock strips |
  | 16 px | tabs, rows, buttons with text, menus |

- **Role colours.** Six roles, each with a dark and a light value ([Icon role colours](#icon-role-colours)). The hexes written in the SVG files are fixed role keys, and a swap on load maps each key to the theme's value, in both themes. No SVG is edited for the theme.
- **Icon state** shows on the button, never in the icon's colours: `fill.hover` when hovered, the accent subtle fill when on. A disabled icon is a greyscale copy, faded once with its button by `opacity.disabled`.
- **The Glyph set:** chevron, tick, cross, minus, close ×, arrow, modified dot, the window controls, the search glass in fields, the lock and funnel toggles in list headers and rows, and the warning triangle and severity shapes (which take the semantic colours).
  - Glyphs live as one named set in `widgets`. Each is drawn as 1.5 px round-capped strokes in a 16 px box, snapped to the pixel grid. The box is fixed like an icon's.
  - The `search`, `lock` and `close` Icons stay for their Commands only.

## Components

Each recipe below names the tokens a component uses per state. They are locked as drawn in the component gallery ([Images](#images)).

### States shared by every control

- **Hover:** `fill.hover` over what is there.
- **Press:** `fill.press` in its place.
- **Keyboard focus:** a 2 px ring in `stroke.focus_ring`, 1 px outside the control, following its radius.
- **Disabled:** the whole widget at `opacity.disabled`.

### Buttons

- **Filled at rest, no border:** `fill.control`, control radius, `control_x` × `control_y` padding, body text. An icon sits 6 px before the label.
- **Primary:** the accent solid with its on-colour. One per dialog.
- **Destructive:** the error solid with its on-colour, for the confirm button of a destructive dialog.
- **Toggle:** when on, the accent subtle fill.
- **Flat:** no fill until hovered.
- **Icon button:** a square of the control height holding one Icon, flat at rest, accent subtle when on.
- **Link:** accent text, underlined when hovered or focused.

### Fields

- **Text field:** `surface.well` with a 1 px `stroke.strong` outline and the control radius.
  - Hover darkens the outline.
  - Focus swaps it for the focus ring.
  - Invalid swaps it for a line in the error text colour, with a cross and a message in the error text colour.
- **Number field:** the numeric style, with the unit in muted text.
- **Search field:** the search glass Glyph in muted text at the leading end.
- **Combo box:** filled like a button, with a chevron Glyph. Its list is a menu.

### Check box, radio and switch

- **Marks are 16 px.** A check box has the mark radius, and a radio is round. Off is `surface.well` with a `stroke.strong` outline. On is the accent solid with the tick, minus or dot in its on-colour.
- **Switch:** a 28 × 16 px track with a white 12 px knob. The track is the accent solid when on.
- **Which one:** a switch is for a setting that applies the moment it is flipped. A check box is for a choice confirmed with a button, and for lists and tables.
- **Segmented control:** for a short set of exclusive choices that apply when picked. The track is filled like a button, at the control height and radius. Segments are equally wide (the widest label plus the control padding) and sit 2 px inside the track, with a radius 2 px smaller.
  - The chosen segment takes the accent subtle fill, as a toggle that is on does. Text is the primary colour on every segment.
  - Hover and press lay the neutral fills on one segment. The focus ring goes round the focused segment.
- **Stepper field:** a number field for a whole number, with a 20 px column at its trailing end, set off by a hairline and split into an up and a down half. Each half holds a chevron Glyph in muted text.
  - A half takes `fill.hover` and `fill.press` like anything flat. A half that cannot step further is drawn at `opacity.disabled`.
  - Up and Down step while the field has the keyboard. A typed number applies on Enter or when the field loses the keyboard, and is kept inside the range.
- **Filterable dropdown:** for a long list. Closed, it is a dropdown. Its list starts with a search field that has the keyboard as soon as the list opens, and typing narrows the rows to those that contain the text.
  - Up and Down move the lit row, which takes `fill.hover`. Enter picks it, and Escape closes the list.
  - The chosen row has the tick. A row can carry a note in muted text at its trailing end, and a hairline can set the first row apart while nothing is typed.
  - When nothing matches, the list says so in one line of muted text.
- These three were not in the component gallery. They are drawn in the widget gallery ([Enforcement and tests](#enforcement-and-tests)).

### Menus and context menus

- A popup: `surface.floating`, popup radius, `elevation.popup`, with `menu` padding around the items.
- Items are rows with the row radius. Hover is `fill.hover`.
- An icon column at the leading end. A checked item shows a tick there in place of an icon.
- Shortcuts in muted text at the trailing end, and a muted chevron for a submenu.
- Separators are hairlines inset by `row_x`.

### Tabs

- **A tab group** is a strip of tabs on the base over a card. The first tab starts 6 px in from the card's edge.
- **The front tab** of a group is a card joined to its panel: `surface.card`, with no line between tab and panel. Its top corners are the card radius less 2.
- **The active Document's tab** adds the 2 px accent line along its top edge, between the rounded corners, and the `body_strong` style. The line means "menus and Commands act on this Document". It stays while keyboard focus is in a Panel, and a Panel's tab never gets one.
- **Back tabs** sit on the base in muted text, with `fill.hover` as a rounded pill when hovered.
- Each tab shows its 16 px Icon in role colours, its title, and then:
  - a **modified dot,** which gives way to the close × on hover;
  - the **close ×** on the front or hovered tab;
  - the **out-of-sync badge:** a warning triangle in the warning solid.
- Padding is `tab_x` × `tab_y`.
- **While a tab is dragged,** the place it would land is a drop target: the accent subtle tint inside a 2 px accent line, with the card radius. The drop-target button under the pointer is drawn the same way. A drop the layout would refuse shows the error tone instead, with the reason beside the pointer.

### Docks

- An expanded Dock is a column of cards with no header.
- A collapsed Dock is a strip of 20 px Icon buttons on the base, with no frame, one control wide. Its first button, a chevron, expands the Dock. The Icon of the open flyout is on.
- A flyout is the Dock's card floating over the window beside its strip:
  - the card's surface, rim, radius and `panel` padding, under `elevation.popup` because it floats;
  - a card gap from the strip, and as long as the strip (a bottom Dock's is as wide as its strip);
  - a title row first: the Panel's Icon and its name in the `title` style. The strip shows Icons only, so the flyout names what it holds.
- The gap at a Dock's inner edge is its splitter. It shows `fill.hover` while hovered and the accent while dragged.

### Panels

- A Panel's card has `panel` padding.
- **A section header** is a row: a chevron, the title in the `title` style, and a muted count.
- **Property rows:** names in muted text, values lined up in one column, `stack` apart.

### Lists, trees and tables

- **Rows** have the row radius and sit inside the card's padding. There are no stripes and no rules between rows.
- **Trees** indent 16 px per level.
- **Table headers** are small muted text over one hairline.
- **Number columns** are right-aligned in the numeric style.
- **Severity** is a shape and a colour. A waived row is muted.
- **Selection** as in [Selection and window focus](#selection-and-window-focus).
- **A drop target** (a row something dragged would drop on) is the accent subtle tint inside a 2 px accent line.
- **Swatches** for data colours are 14 px squares with the mark radius and a neutral rim.
  - Something with no colour set (a Net without one) shows the same square, hollow, with a slash.
  - A swatch that edits its colour is a button that opens the colour picker: a field of saturation and brightness, a strip of hues, and the colour as `#rrggbb` to type, with a copy button.
- **Badges and counts** are pills in small text on a tone's subtle tint.
  - A badge names a state or a tally, and can lead with the status mark of its tone. Without a tone it is muted text on the neutral control fill.
  - A count is a number alone on the neutral fill, in tabular digits.
  - A badge that filters a list (Messages' Errors and Warnings) is a toggle. Off, it is neutral and muted, and keeps its size.
- **Scrollbars** are a 4 px muted thumb with no track, widening to 8 px when hovered.

### Dialogs and settings windows

- **A dialog:** `surface.floating`, window radius, `elevation.window`, `dialog` padding.
  - A title row in the `title` style with a close ×.
  - Buttons right-aligned in one row, `gap` apart and `section` below the content, with the one primary button last. Cancel comes before it, and any third choice before Cancel.
  - A destructive confirm starts with keyboard focus on Cancel.
- **Modals** sit over `surface.scrim`.
- **Form rows:** the label at the left and the control at the right, `stack` apart, with sections `section` apart. A caption is one line of small muted text under its setting.
- **Settings windows** have a sidebar on the base colour holding the page tree, and their page on the floating surface.

### Tooltips

- A popup holding either a name and a key chip, or a title and a sentence in small muted text. It is smaller than a menu: the control radius and a control's padding.
- **A key chip** is small muted text on `fill.control` with the mark radius.

### Status bar

- On the base, with no rule above it. Small muted text, with the numeric style for coordinates and deltas.
- Its items are flat buttons with the row radius, side by side with no separators. A note may take a semantic colour's `text` strength.
- The active Layer's item shows the Layer's swatch before its name.

### Command Palette

- A window (`surface.floating`, `elevation.window`) dropped from the command field.
- Its rows are menu rows. The selected row is the live selection, and shortcuts are key chips.

### Bars on the canvas

- The Active Bar and a Tool's bar are floating bars: `surface.floating`, popup radius, `elevation.popup`, holding 20 px Icon buttons 2 px apart.
- The active Tool's button is on (accent subtle).

### Progress and sliders

- **Progress bar:** a 6 px rounded rail in `fill.control`, filled with the accent solid. Its final state turns to the success or error solid.
- **Slider:** a 4 px rail filled with the accent solid up to a white 16 px knob with a `stroke.strong` rim.

## Title bar

What the Title bar holds and how it behaves is in [app.md](app.md#window-shell) and [ADR-0014](../../adr/0014-reda-draws-its-own-title-bar.md). Its look:

- **On the base colour,** with no line under it. The Title bar and the cards below read as one surface.
- **Height** follows Density: 40 px Comfortable, 36 px Compact at 13 px text.
- **Quick strip:** 20 px Icon buttons of the control height, 2 px apart.
- **Menus:** flat buttons in body text.
- **Command field:** a filled control centred in the window, up to 420 px wide and at least 180 px, with the search glass Glyph in muted text and its key in small muted text when there is room. At rest it shows "Project name — active Document" in body text; with no Project open it shows "Search commands" in muted text. Where it cannot be centred it moves aside before it shrinks.
- **Window controls** (Windows and Linux): minimise, maximise or restore, and close. Each is 46 px wide and the full height of the bar, flat, with a 1.5 px Glyph. Hover is `fill.hover`, except close, whose hover is `color.close_hover` with a white Glyph.
- **macOS:** the native traffic lights at the left, vertically centred. The quick strip starts after their 78 px inset.
- **Narrow windows:** a drag gap of at least 48 px is kept beside the window controls while the bar drags the window; with the system title bar the gear keeps only the bar's 6 px padding. The command field shrinks to its minimum, then becomes an icon button. Then the menus collapse into a single menu button, and the field comes back when that frees the room.
- **Inactive window:** text and window-control Glyphs go muted.
- **Window corners** take the window radius. They are square when the window is maximised or uses the system title bar.
- **Floating windows:** a 32 px bar on the base with the window's title at the left in body text, and the same window controls. On macOS the bar takes the native title-bar height.
- **Edge-resize strip:** an invisible hit strip inside the window edge on Windows and Wayland, off while maximised: 6 px along the edges and 12 px at the corners, along each of the two edges that meet there.

## Contrast

The rules are a table beside the tokens: foreground, ground and required ratio. One test walks it for light and dark. A second test fails if a colour token appears in no rule.

| What | On what | Ratio |
|---|---|---|
| `text.primary`, `text.muted` and each tone's `text` | base, card and floating, plain and under `fill.hover` | 4.5:1 |
| `text.primary` | a filled control on each surface, hovered and pressed | 4.5:1 |
| `text.primary`, `text.muted` | `surface.well` | 4.5:1 |
| Each tone's `on_solid` | its solid, plain, hovered and pressed | 4.5:1 |
| `text.primary`, `text.muted` | each tone's subtle tint on each surface, plain and hovered | 4.5:1 |
| Each tone's `text` | its own subtle tint on each surface | 4.5:1 |
| `text.primary`, `text.muted` | `fill.selected_unfocused` on each surface | 4.5:1 |
| The accent line and the focus ring | base, card and floating | 3:1 |
| The white Glyph | `color.close_hover` | 4.5:1 |
| Each icon role colour | base, card and floating, at rest, hovered and toggled on | 3:1 |
| `touch` | base, card and floating | 3:1 |
| `stroke.strong` | base, card, floating and `surface.well` | 1.5:1 |
| `surface.floating` | base and card under `surface.scrim` | 1.25:1 |
| `stroke.hairline` | base, card and floating | 1.15:1 |

- That is 129 rules per theme before the icon rows, which add six roles × nine grounds, and the last four rows, which add twelve. None fail with the values below.
- The last four rows are there so that every colour token has a rule. They are floors for things that are meant to be quiet (an outline, a hairline, a dialog over the dimmed window), not text ratios.
- An icon's momentary pressed state is exempt.
- The icon rule replaces the earlier test of the blue on the active button background.

## Enforcement and tests

- **A source scan.** A test in `reda-ui` reads every `.rs` file under `crates/reda-ui/src` as text. It is written in-house, with no new dependency and no clippy configuration. It lives in `crates/reda-ui/tests/source_scan/`, and its lists (exempt modules, stock widgets) are in `lists.rs` there.
  - **Banned outside the exempt modules:**
    - colours: `Color32::from_*` and named constants, `gamma_multiply`, `linear_multiply`, hex literals;
    - egui style access: `ui.visuals()`, `ui.spacing()`, `ui.style()` and their `_mut` forms, `ctx.set_style*`, `set_visuals*`;
    - sizes with a numeric literal: `add_space(<n>)`, `CornerRadius`, `Margin`, `Stroke::new(<n>, ..)`, `FontId::new`, `.size(<n>)`, `Shadow`, and a literal `vec2` in spacing and min-size calls;
    - a raw stock widget once its REDA widget exists (`ui.button(`, `Button::new`, `TextEdit::`, `ui.checkbox(` and so on).
  - **Allowed everywhere:** zero (`Color32::TRANSPARENT` counts as zero), and arithmetic on token values.
  - **Exempt modules:** `theme/`, `widgets/` and the on-canvas drawing modules, listed by path in the test.
  - **Marker:** `// theme-exempt: <reason>` exempts the next line. A marker with no reason fails, and the test prints the total.
  - **Every file is converted.** The migration's list of files not yet converted is gone: every file the scan reads must be clean.
  - The scan works on text, so an alias or a macro can defeat it. It is there to stop accidents.
- **The contrast test,** as above.
- **The widget gallery.** A test-only gallery draws every REDA widget in every state (rest, hover, press, focus, disabled, selected, invalid) and the Glyph set, in the four variants, and is snapshotted.
- **Whole-screen snapshots** stay in dark Comfortable only, apart from the whole-app screen of [Images](#images), and are drawn in a 1280 × 800 window, where two side Docks at their opening width leave the canvas and the editors room for what each scene shows.
- **Fit tests.** One per area of the window (`crates/reda-ui/tests/*_fit.rs`, and `panel_sheets.rs` for the Panels, their Docks and the status bar) drives its screens in light and dark, Comfortable and Compact, at the default text size and at both ends of the range, and fails on text cut short with an ellipsis, text that runs out of its clip, and a label broken over lines.
- **Font coverage.** `crates/reda-ui/tests/font_coverage.rs` fails when the message catalogue, a Command's or a Preference's text, or a string literal of a crate whose messages reach the window holds a character the bundled faces lack.

## Values

The tables in this section are the values of `Tokens`, generated from it. They began as the resolution of [What does each component look like in the chosen direction?](https://github.com/openreda/reda/issues/276) and its prototype; the tickets on motion and icons supplied the motion and icon tables.

<!-- tokens:begin -->
<!-- Generated from `Tokens` (crates/reda-ui/src/theme) by `cargo xtask tokens`. Do not edit between the markers. -->

### Colour values

| Token | Light | Dark |
|---|---|---|
| `surface.base` | `#F0EEE6` | `#1F1E1D` |
| `surface.card` | `#FAF9F5` | `#262624` |
| `surface.floating` | `#FDFCFA` | `#2C2B29` |
| `surface.well` | `#FFFFFF` | `#1A1918` |
| `surface.scrim` | black at 35 % | black at 55 % |
| `text.primary` | `#1F1E1C` | `#F3F1EA` |
| `text.muted` | `#59574F` | `#C2C0B7` |
| `stroke.hairline` | `#DEDBD0` | `#3B3A37` |
| `stroke.strong` | `#C4C0B3` | `#55534E` |
| `stroke.focus_ring` | `#1F5FBF` | `#8FBAF8` |
| `fill.control` | `text.primary` at 7 % | `text.primary` at 8 % |
| `fill.hover` | `text.primary` at 5.5 % | `text.primary` at 7 % |
| `fill.press` | `text.primary` at 11 % | `text.primary` at 13 % |
| `fill.selected_unfocused` | `text.primary` at 8 % | `text.primary` at 10 % |
| `touch` | `#2F7D4F` | `#6FBF8A` |
| `close_hover` | `#C4392F` | `#C4392F` |

| Tone | Strength | Light | Dark |
|---|---|---|---|
| `accent` | text | `#1B55AD` | `#8FBAF8` |
| | subtle | `#1F5FBF` at 14 % | `#4F8FE8` at 22 % |
| | solid | `#1F5FBF` | `#2458B0` |
| | on_solid | `#FFFFFF` | `#FFFFFF` |
| `error` | text | `#A8231B` | `#F5998C` |
| | subtle | `#C8372D` at 13 % | `#E5534A` at 20 % |
| | solid | `#B3261E` | `#AD2F27` |
| | on_solid | `#FFFFFF` | `#FFFFFF` |
| `warning` | text | `#7A5200` | `#E5B04A` |
| | subtle | `#D99A00` at 20 % | `#E5A530` at 16 % |
| | solid | `#F0B429` | `#E5B04A` |
| | on_solid | `#2A1F00` | `#231A05` |
| `success` | text | `#1F6139` | `#7CC792` |
| | subtle | `#2E8B57` at 15 % | `#4FAF6E` at 18 % |
| | solid | `#26734A` | `#25683F` |
| | on_solid | `#FFFFFF` | `#FFFFFF` |

### Icon role colours

| Role | Key in the SVG | Light | Dark |
|---|---|---|---|
| Context (what already exists) | `#8a93a0` | `#79766e` | `#96918a` |
| Schematic | `#2fb36b` | `#228a51` | `#2fb36b` |
| Copper | `#e08a2e` | `#ac6921` | `#e08a2e` |
| General | `#3d8fd6` | `#347dbc` | `#3d8fd6` |
| Picked point | `#ec4f7a` | `#d2456c` | `#ef5a82` |
| Board edge and Keepout | `#d4c03c` | `#8a7a12` | `#d4c03c` |

The dark picked-point value is the one closest to `#ec4f7a` that the contrast test accepts.

### Space values

The only tokens that follow Density. Pixels at UI scale 100 %.

| Token | Job | Comfortable | Compact |
|---|---|---|---|
| `control_x` | Inside buttons, fields and combo boxes, sideways | 12 | 10 |
| `control_y` | The same, vertically | 5 | 3 |
| `row_x` | Inside rows of lists, trees, tables and menus, sideways | 8 | 6 |
| `row_y` | The same, vertically | 4 | 2 |
| `tab_x` | Inside a tab, sideways | 12 | 10 |
| `tab_y` | The same, vertically | 7 | 5 |
| `gap` | Between controls side by side | 8 | 6 |
| `stack` | Between stacked rows of a form | 8 | 4 |
| `section` | Between sections | 16 | 12 |
| `panel` | Inside a Panel's card | 12 | 8 |
| `dialog` | Inside a dialog | 20 | 16 |
| `menu` | Inside a menu or popup, around its items | 6 | 4 |
| `bar_y` | Above and below the Title bar's controls | 5 | 5 |

### Type values

B is the body size from `general.text_size` (default 13). Line height is `2 * floor(0.75 * size + 0.5)`.

| Style | Size | Weight | Size / line at B = 13 |
|---|---|---|---|
| `small` | B − 2, never below 10 | regular | 11 / 16 |
| `body` | B | regular | 13 / 20 |
| `body_strong` | B | medium | 13 / 20 |
| `title` | B | semibold | 13 / 20 |
| `heading` | B + 3 | semibold | 16 / 24 |
| `mono` | B − 1, on the body's line height | regular | 12 / 20 |
| `numeric` | B | regular, tabular digits | 13 / 20 |
| `numeric_small` | B − 2, never below 10 | regular, tabular digits | 11 / 16 |

| B | `small` | `body` | `heading` | `mono` | Control height (Comfortable / Compact) |
|---|---|---|---|---|---|
| 11 | 10 / 16 | 11 / 16 | 14 / 22 | 10 / 16 | 26 / 22 |
| 12 | 10 / 16 | 12 / 18 | 15 / 22 | 11 / 18 | 28 / 24 |
| 13 | 11 / 16 | 13 / 20 | 16 / 24 | 12 / 20 | 30 / 26 |
| 14 | 12 / 18 | 14 / 22 | 17 / 26 | 13 / 22 | 32 / 28 |
| 15 | 13 / 20 | 15 / 22 | 18 / 28 | 14 / 22 | 32 / 28 |
| 16 | 14 / 22 | 16 / 24 | 19 / 28 | 15 / 24 | 34 / 30 |

### Size values

Widths that follow the text size and never Density. Each is given at B = 13 and scaled by B / 13 above it (`size.of(..)` in code), on whole pixels. Below 13 they keep their 13 px width, since icons, marks and paddings do not shrink.

| B | Scale | `size.dock` | `size.field` |
|---|---|---|---|
| 11 | 1.00 | 288 | 280 |
| 12 | 1.00 | 288 | 280 |
| 13 | 1.00 | 288 | 280 |
| 14 | 1.08 | 310 | 302 |
| 15 | 1.15 | 332 | 323 |
| 16 | 1.23 | 354 | 345 |

### Fixed values

| Token | Value |
|---|---|
| `radius.mark` | 4 |
| `radius.row` | 6 |
| `radius.control` | 8 |
| `radius.card` | 12 |
| `radius.popup` | 12 |
| `radius.window` | 16 |
| Hairline width | 1 |
| Accent line width | 2 |
| Focus ring width | 2 |
| Icon size: tabs, rows, buttons, menus | 16 |
| Icon size: quick strip, Tool bars, Active Bar, Dock strips | 20 |
| Glyph box and stroke | 16, 1.5 |
| `opacity.disabled` | 45 % |
| Gap between cards, and from cards to the window edge | 6 |
| Check box and radio mark | 16 |
| Switch track and knob | 28 × 16, 12 |
| Swatch | 14 |
| Tree indent per level | 16 |
| Window control width | 46 |
| Drag gap kept beside the window controls | 48 |
| Edge-resize strip, on edges and at corners | 6, 12 |
| Command field width | 180 to 420 |
| Scrollbar thumb, at rest and hovered | 4, 8 |
| Progress rail and slider rail | 6, 4 |

The rows without a token name are dimensions of single components that no setting changes. Stroke widths and icon sizes are tokens (`stroke`, `icon`). The rest are named constants beside their recipe in `widgets`, such as `widgets::SWITCH_TRACK`, not `Tokens` entries; the scrollbar's are beside the generated `Style`.

### Elevation values

Offset x / y, blur, colour. Spread is zero.

| Token | Light | Dark |
|---|---|---|
| `elevation.card` | 0 / 1, blur 4, black at 9 % | 0 / 1, blur 4, black at 27 % |
| `elevation.popup` | 0 / 6, blur 24, black at 10 % | 0 / 6, blur 24, black at 39 % |
| `elevation.window` | 0 / 12, blur 40, black at 12 % | 0 / 12, blur 40, black at 47 % |

### Motion values

`motion.fast` is 90 ms and `motion.base` is 140 ms. Milliseconds in / out; ease-out unless noted.

| Transition | In | Out | What moves |
|---|---|---|---|
| Hover fill, scrollbar widening | at once | 90, linear | the fill fades out; the thumb narrows |
| Press fill | at once | 90 | fades out |
| Check box, radio, toggle on | 90 | 90 | the fill fades, the mark grows |
| Switch | 140 | 140 | the knob slides, the track colour mixes |
| Tab accent line | 90 | at once | fades in on the new tab |
| Menu, context menu, combo list | 90 | at once | fade + 4 px from the anchor |
| Tooltip | 90 | at once | fade + 4 px, after a 500 ms wait |
| Flyout | 140 | 90 | fade + 8 px from the strip |
| Dock collapse | 140 | 140 | width; contents clipped and cross-faded |
| Panel section | 140 | 90 | rows revealed from the top, the chevron turns |
| Dialog and scrim | 140 | 90 | fade, grows from 97 %; the scrim fades with it |
| Command Palette | 140 | 90 | fade + 4 px drop from the command field |
| Selected row | at once | at once | |
| Bars on the canvas | 90 | 90 | a Tool's bar: fade + 4 px; an on state fades |
| Progress bar | 140 | | catches up with its value |

| Other | Value |
|---|---|
| Travel | 4 px (a flyout: 8 px) |
| Dialog scale at the start | 97 % |
| Tooltip wait | 500 ms |
| Tooltip shown at once after another | within 400 ms |

<!-- tokens:end -->

### Keeping the values in step

- `cargo xtask tokens` generates the tables between the two markers from `Tokens`. A test beside the tokens (`crates/reda-ui/src/theme/tables.rs`) fails when the region is stale, so CI does too. This follows the icon set's README.
- Code is the authority for values, and this file for intent. A value changes in `Tokens`, and the tables follow.
- The region was first written by hand. The first generated version reproduced it exactly.

## Images

The spec embeds only images that tests produce: the widget gallery's pages and one whole-app screen, each in the four variants. They are the snapshots in `crates/reda-ui/tests/snapshots/`, compared in CI, so they change only when a slice rebaselines them; no image is copied into `docs/`.

- The gallery is `crates/reda-ui/tests/widget_gallery/`, one file per page (`gallery_<page>_<theme>_<density>.png`).
- The whole-app screens are drawn by `crates/reda-ui/tests/app.rs` (`whole_app_<theme>_<density>.png`) in the window every whole-screen snapshot uses, 1280 × 800.
- Motion cannot be shown in stills. Run REDA with Animate on (Preferences ▸ Appearance); `crates/reda-ui/tests/motion.rs` checks the timings.

### The whole app

| Light | Dark |
|---|---|
| ![The whole app, light, Comfortable](../../../crates/reda-ui/tests/snapshots/whole_app_light_comfortable.png) | ![The whole app, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/whole_app_dark_comfortable.png) |
| ![The whole app, light, Compact](../../../crates/reda-ui/tests/snapshots/whole_app_light_compact.png) | ![The whole app, dark, Compact](../../../crates/reda-ui/tests/snapshots/whole_app_dark_compact.png) |

### Controls

Buttons, fields, dropdowns, check box, radio, switch, slider, progress, segmented control, stepper, the Glyph set.

| Light | Dark |
|---|---|
| ![Controls, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_controls_light_comfortable.png) | ![Controls, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_controls_dark_comfortable.png) |
| ![Controls, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_controls_light_compact.png) | ![Controls, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_controls_dark_compact.png) |

### Menus and popups

Menus, tooltips, the dialog frame, cards, the Dock flyout.

| Light | Dark |
|---|---|
| ![Menus and popups, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_popups_light_comfortable.png) | ![Menus and popups, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_popups_dark_comfortable.png) |
| ![Menus and popups, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_popups_light_compact.png) | ![Menus and popups, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_popups_dark_compact.png) |

### Lists, trees and tables

Rows, section headers, form rows, status marks, swatches, badges.

| Light | Dark |
|---|---|
| ![Lists, trees and tables, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_data_light_comfortable.png) | ![Lists, trees and tables, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_data_dark_comfortable.png) |
| ![Lists, trees and tables, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_data_light_compact.png) | ![Lists, trees and tables, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_data_dark_compact.png) |

### The window

Tabs, a tab group's card, the status bar, a dragged tab.

| Light | Dark |
|---|---|
| ![The window, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_shell_light_comfortable.png) | ![The window, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_shell_dark_comfortable.png) |
| ![The window, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_shell_light_compact.png) | ![The window, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_shell_dark_compact.png) |

### The Title bar

Window controls, the command field, a floating window's slim bar.

| Light | Dark |
|---|---|
| ![The Title bar, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_title_bar_light_comfortable.png) | ![The Title bar, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_title_bar_dark_comfortable.png) |
| ![The Title bar, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_title_bar_light_compact.png) | ![The Title bar, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_title_bar_dark_compact.png) |

### Bars on the canvas

The Active Bar, the Command Palette, the keys card.

| Light | Dark |
|---|---|
| ![Bars on the canvas, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_bars_light_comfortable.png) | ![Bars on the canvas, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_bars_dark_comfortable.png) |
| ![Bars on the canvas, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_bars_light_compact.png) | ![Bars on the canvas, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_bars_dark_compact.png) |

### Canvas chrome

Layer chips, the Layer swatch, the Layer strip.

| Light | Dark |
|---|---|
| ![Canvas chrome, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_canvas_chrome_light_comfortable.png) | ![Canvas chrome, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_canvas_chrome_dark_comfortable.png) |
| ![Canvas chrome, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_canvas_chrome_light_compact.png) | ![Canvas chrome, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_canvas_chrome_dark_compact.png) |

### Text to read

The read-only text view.

| Light | Dark |
|---|---|
| ![Text to read, light, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_text_view_light_comfortable.png) | ![Text to read, dark, Comfortable](../../../crates/reda-ui/tests/snapshots/gallery_text_view_dark_comfortable.png) |
| ![Text to read, light, Compact](../../../crates/reda-ui/tests/snapshots/gallery_text_view_light_compact.png) | ![Text to read, dark, Compact](../../../crates/reda-ui/tests/snapshots/gallery_text_view_dark_compact.png) |
