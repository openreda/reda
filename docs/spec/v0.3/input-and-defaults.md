# v0.3 Wheel mapping, arrow keys and Object Defaults

Source: [How do wheel mapping, the arrow-key cursor and object defaults work?](https://github.com/openreda/reda/issues/438), after the survey in [How do other EDA tools handle grids, guides, wheel mapping, cursor keys and object defaults?](https://github.com/openreda/reda/issues/428) ([findings](https://github.com/openreda/reda/blob/research/grids-guides-input/docs/research/grids-guides-input.md)).

This file says what the wheel does and how it is remapped, what the arrow keys do on a canvas, and where a placing Tool gets its starting values. It extends the v0.1 [navigation](../v0.1/editing.md#navigation) and [keymap](../v0.1/app.md) rules, which stand unless changed here. Crates: `reda-engine` (Preferences), `reda-ui` (canvas input, Tools, the Preferences pages).

## Wheel mapping

A Preferences page, **Mouse Wheel**, holds a table of wheel actions. Each action is bound to one modifier combination: none, or any of Shift, Ctrl and Alt together.

| Action | Editors | Default |
|---|---|---|
| Zoom at the cursor | all | plain wheel |
| Scroll sideways | all | Shift |
| Scroll up and down | all | Ctrl |
| Step Layer | PCB | unbound |

- **No clashes:** two actions can't share a combination. The page refuses the second and names the action that holds it.
- **Presets:** a dropdown fills the table.
  - **REDA** is the table above.
  - **Wheel scrolls** has the plain wheel scroll up and down, Shift scroll sideways and Ctrl zoom, for users coming from the leading suite.
  - An edited table reads **Custom**.
- **One mapping** per user, for every editor and for the [3D view](3d-view.md). It is stored as `canvas.wheel.*` Preferences.
- **Zoom speed** and **Reverse zoom direction** are on the same page.
- **Touchpad:**
  - **Auto** (default): a scroll the system reports as coming from a touchpad pans the view, and a pinch zooms. A scroll from a wheel, or of unknown source, follows the table.
  - **Always pan** treats every scroll as a touchpad's.
  - **Same as wheel** treats every scroll as a wheel's.
- **Drag buttons are fixed.** Right-drag and middle-drag pan, and the 3D view's turning drag stays as [specified](3d-view.md). They are not on this page.

## Arrow keys

| Keys | Does |
|---|---|
| Arrow | Moves the Edit Cursor one grid step |
| Shift+arrow | Moves it ten steps |
| `Mod`+arrow | Nudges the selection one step |
| Shift+`Mod`+arrow | Nudges it ten steps |

All four are Commands, so a user who wants plain arrows to nudge can swap the keys.

Keys on the digit row are matched by physical key, in 2D as in 3D ([3D view keys](3d-view.md#keys)), so a Keymap binding on a digit works on every keyboard layout.

### The Edit Cursor

The Edit Cursor is the snapped point where a Tool acts. It follows the mouse pointer, and the arrow keys step it. v0.1 used "cursor" for both the pointer and this point.

- **Where:** the PCB, Footprint, Schematic and Symbol editors, idle and with a Tool in flight. An object in flight moves with it, which is how a Part or a corner is placed an exact number of steps away. A focused text field keeps the arrows for itself.
- **The pointer moves too.** The Preference **Arrow keys move the mouse pointer** (on) moves the system pointer with the Edit Cursor, so the two never part.
  - Where the desktop does not let an application move the pointer, the box is shown ticked and disabled with a caption, as the title bar Preference is, and the stored value is left alone.
  - With the Preference off, or on such a desktop, the Edit Cursor leaves the pointer behind. It stays where the keys put it until the mouse moves more than a few pixels, then follows the pointer again.
- **Leaving the view:** the view follows under the [Auto-pan](../v0.1/editing.md#navigation) Preference.
- **Home** re-centres the view on the Edit Cursor.

### One step

- A step is the grid in use under the Edit Cursor, per axis where [X ≠ Y](grids-guides.md#grid-x--y).
- Inside a [Local Grid](grids-guides.md#local-grids) the step is that grid's. On a polar one, Left and Right move along the radius and Up and Down along the angle.
- From a point off the grid, the first press lands on the next grid line in that direction. Later presses step from there.
- Arrow steps ignore Hotspots, Guides and Alignment Guides, because the keys are for an exact count of steps.

### Clicking by key

`view.cursor.click` acts as a left click at the Edit Cursor.

- **Enter** runs it wherever the running Tool has not bound Enter. Enter keeps its v0.1 meaning where a Tool has: it ends a route, closes a Zone, Keepout, Board Outline, Board Cutout or polygon, and finishes a sheet polyline.
- **Shift+Enter** runs it everywhere, so a route corner or a polygon vertex can be placed by keys.

### Nudging

- `Mod`+arrows nudge the selection in all four editors. v0.1 specified it once, and only the Schematic editor has it.
- The step follows [One step](#one-step), measured at the selection's first object.
- One press is one undo step. Connections rubber-band as in a drag. Locked objects stay put under the [lock rules](../v0.1/pcb.md#locks).

## Object Defaults

Each kind of object has one stored record of starting values, its Object Defaults. A placing Tool starts from the record of the kind it places.

- **Per user.** The records are Preferences, under `defaults.*` in `preferences.toml`. Nothing is stored in a Project or a board. A [Board Template](../v0.2.5/template-format.md) is how a board carries its own starting content.
- **Properties of the object only.** A record never holds position, identity, Net or text content. It never holds a value the board decides through Design Rules, Net Classes or Via types, so Track width, Via size and clearances are not in it. Tool behaviour is not in it either: the route width mode, Via size mode, Wire mode, Arc mode, rotation in flight and auto-increment stay where v0.1 and v0.2 put them.
- **Tools only.** Commands take explicit values and never read a record, so an MCP agent or the CLI gets the same result on every machine.

### Changing a value while placing

- Every placing Tool shows its record as Tool options in Properties while it is active, as the Zone Tool does in v0.1. Tab moves the keyboard there.
- **Remember last used** (one Preference, on): an edit made there is written to the record, so the next placement and the next session start from it. With it off, an edit lasts until the Tool is left and the record changes only on the Preferences page.

### The Defaults page

- **Preferences ▸ Defaults** lists the kinds grouped by editor, with a filter. Choosing a kind shows its fields, generated from the object schema as [Select Similar](../v0.1/editing.md#select-similar)'s rows are. A value keeps the unit it was typed in.
- **Reset** restores the shipped values, for one kind or for all.
- **Save As…** writes a TOML file holding the `defaults.*` table, for all kinds or for the chosen kind. It is the same text as in `preferences.toml`, so it can be read, compared and edited by hand.
- **Load…** replaces the records of the kinds the file holds and leaves the others alone. Entries it does not know or cannot read are skipped and counted in a message after the load. A load never fails part-way.
- Saving and loading cover Object Defaults only, not the rest of Preferences.

### Kinds and fields

| Editor | Kind | Fields | Shipped values |
|---|---|---|---|
| PCB, Footprint | Line, Rectangle, Arc, Circle, Polygon | width, fill | [per Layer kind](#per-layer-kind); not filled |
| PCB, Footprint | Text | height, thickness, font, justify | 1 mm, 0.15 mm, the stroke font, left and bottom on a board, centred in a Footprint |
| PCB | Dimension | line and extension widths, offset, gap, text gap, arrow size and length, arrow position, precision, units, format, text height | 0.15 mm, 2 mm, 0.5 mm, 0.25 mm, 1 mm and 2 mm, inside, 2, automatic, value and unit, 1 mm |
| PCB | Free Pad | kind, shape, size, hole | plated through-hole, circle, 2 mm, 1 mm |
| PCB | Zone | clearance, minimum width, pour over, remove unconnected, minimum island area | unset, unset, Zones only, on, 0 |
| PCB | Keepout | shape, all Layers, restrict | polygon, on, everything but Footprints |
| PCB | Via | teardrop | on |
| Footprint | Pad | kind, shape, size, hole, chamfer, margins | SMD, rectangle, 1 × 1.5 mm, 0.8 mm when through-hole, 50 %, none |
| Symbol | Pin | length, type, name visible, number visible | 100 mil, Passive, on, on |
| Symbol | Line, Rectangle, Arc, Circle, Polygon | width, fill | 10 mil, not filled |
| Symbol, Schematic | Text | height | 50 mil |
| Schematic | Rectangle | fill | not filled |
| Schematic | Port | direction | bidirectional |

- **No record:** Track, Wire, Net Label, Power Port, No-Connect, Board Outline, Board Cutout and placed Parts. Their values come from the rules, the Library or the name.
- **Moved in:** the v0.1 Zone Tool values (`pcb.zone.tool.*`), the Keepout values and the Footprint editor's Pad template become these records. The old keys go.

### Per Layer kind

On boards and in Footprints, the Line-family and Text records hold one set of values per Layer kind, because a Courtyard line and a Silkscreen line should not share a width. The two editors share the records, so a Silkscreen line starts the same on a board and in a Footprint.

| Layer kind | Line width | Text |
|---|---|---|
| Copper | 0.15 mm | 1 mm high, 0.15 mm thick |
| Silkscreen | 0.15 mm | the same |
| Courtyard | 0.05 mm | the same |
| Assembly | 0.1 mm | the same |
| [Mechanical](mechanical-layers.md) | 0.15 mm | the same |

## Commands

| Command | Does |
|---|---|
| `view.cursor.step` | Moves the Edit Cursor by a number of grid steps in X and Y |
| `view.cursor.click` | A left click at the Edit Cursor |
| `pcb.nudge`, `footprint.nudge`, `symbol.nudge` | Nudge the selection, beside the existing `schematic.nudge` |
| `app.defaults.reset / save / load` | Reset, write or read Object Defaults, for all kinds or one |

Single values are read and set with the existing `app.preferences.get` and `app.preferences.set`. The exact Preference keys and Command parameters are settled with the rest of the v0.3 data in [What Commands and file-format additions does the v0.3 data need?](https://github.com/openreda/reda/issues/440).

## Differences from other tools

- **The leading suite's** plain wheel scrolls. REDA's zooms, and one preset gives the other mapping.
- **The leading suite** always moves the mouse pointer with the arrow keys and always treats Enter as a click. REDA moves the pointer where the desktop allows it, and keeps Enter for finishing in the Tools that already use it.
- **The leading suite** lists Track and Via sizes among its defaults. REDA leaves out every value the board's rules decide.
- **The leading suite's** "Permanent" switch is REDA's Remember last used, the other way round.
- **KiCad** keeps text and graphics defaults in the board. REDA's are per user, with a file to pass them on.
- **DipTrace, EasyEDA Pro and Horizon EDA** move the selection with the plain arrows. REDA's plain arrows move the Edit Cursor, and `Mod`+arrows move the selection.
