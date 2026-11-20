# v0.2.5 Board Template format

Source: [Board Template file format, parameter model and where templates live](https://github.com/openreda/reda/issues/396), recorded as ADR [0016](../../adr/0016-board-templates-are-parametric-library-elements.md). It is extended by [how holes and connectors enter the design](https://github.com/openreda/reda/issues/395), the [command surface](https://github.com/openreda/reda/issues/401) (choice `default`), [card-edge fingers](https://github.com/openreda/reda/issues/399) (`edge_connector`), [plated edges](https://github.com/openreda/reda/issues/403) (`plated` marks) and [drawings](https://github.com/openreda/reda/issues/404) (drawing groups).

This file says what a `.redaboard` file holds, how its parameters shape the geometry, where templates live, and what is checked when one loads.

## The file

- **A new `FileKind`, extension `.redaboard`.** It is a full peer of `.redafab` under ADR [0005](../../adr/0005-canonical-json-one-object-per-line.md):
  - canonical JSON with one object per line and tables sorted by UUID;
  - the shared repo-wide `format` integer and its raw-JSON migration chain;
  - `deny_unknown_fields`;
  - a generated schema in `docs/schemas/`.
- "Template" stays out of the extension, and the Command parameter is `board_template`, because `{template}` is reserved for the v0.7 sheet frames.
- **The one exception to ADR 0005:** any numeric geometry field may be an expression string instead of an integer ([parameters](#parameters-and-expressions)). A board created from a template holds only plain integers.

## Where templates live

- **A Library element.** A template is one file in `board-templates/*.redaboard` inside a Library folder, beside `symbols/`, `footprints/` and `parts/`.
  - Templates are found by scanning, with no manifest.
  - Each has a UUID and a `name` that is unique within its Library.
  - `library.element.copy` works on them.
- **Built-ins** ship in the Starter Library ([catalogue](catalogue.md)). User, team and project templates live in any registered Library.
- **Resolution order:** Project, then user, then Starter. Inactive Libraries are not scanned.
- **Who sees what:**
  - New Project sees the user and Starter Libraries.
  - Add PCB Document also sees the Project's Libraries.
  - How the Commands enforce this is in [commands](commands.md#library-visibility).
- **The Library panel** lists templates only inside a Library tab, with Delete, Duplicate and Copy to Library…. There is no template editor in v0.2.5.

## Header

| Field | Meaning |
|---|---|
| `uuid` | the element's identity |
| `name` | unique within the Library. Built-in names refer to the host only, e.g. "Shield for Arduino Uno" ([catalogue](catalogue.md#naming)). |
| `description` | free text. Compatibility wording, trademark acknowledgements, the source of any approximate drawing and advice such as hard gold go here. |
| `category?` | groups the picker. A template without one is listed under "Other". |
| `suggested_layers` | a plain copper Layer count, 2 to 32. There is no Construction or Stackup, because the Fab Preset stays the user's choice. |
| `board?` | board properties the template sets. In v0.2.5 this is only `edge_connector {bevel?, hard_gold?}` ([edge connectors](edge-connectors.md#the-edge_connector-board-property)). |

There is no author or licence field. The Library carries those.

## Parameters and expressions

### Parameter kinds

- **choice:** `{name, kind: "choice", label, default?, options}`.
  - Each option is `{key, label, bind, suggested_layers?}`. `bind` maps names to numeric values (Lengths or angles), and `suggested_layers` overrides the header's.
  - Every option of one choice binds **the same set of names**.
  - `default` is an option `key`. Without it, the first option is the default.
- **length:** `{name, kind: "length", label, default, min, max, unit}`. `unit` is a display hint (`mm`, `mil` or `in`). It does not change how the file is read.
- Parameter `name`s are `snake_case`. `label`s are what the dialog shows.
- Length `min` and `max` are the only declared constraints. There are no custom constraint expressions. Anything else is caught by [validation](#validation) of the evaluated frame.

### Expressions

- **A numeric geometry field** is an integer (nm, or millidegrees for an angle) or an expression string evaluated by `reda-geom`'s `expr.rs`. Arithmetic is exact, and only the result is rounded.
- **Names:** an expression sees one flat `snake_case` namespace, made of the length parameters and the names bound by choice options. All of these names must be unique across the template.
- **Refused:** the current-value `!`, object-attribute references and function calls.
- **Units:** a bare number is read as mm in a length field and as degrees in an angle field, whatever the user's display units are. Explicit units (`10mil`, `0.1in`) are allowed.
- **Example:** an outline vertex at `{x: "width", y: "height - corner_radius"}`.

### `when` gates

Structural variation goes through `when`, never through option bindings. Option bindings stay numeric, apart from `suggested_layers`.

- Any object row may carry `when: {param: [option keys]}`.
- The gate is an AND across parameters and an OR within one list. A row without `when` always exists.
- This is how a template picks which Part a connector uses, which side or rotation it takes, and whether an object exists at all. For example, PCIe has one connector row per lane width, each gated on `lanes`.
- `when` names choice parameters only.
- **The dialog hides a parameter** whose own row-level `when` does not hold, such as the cutouts on the micro Pi board ([dialog](creation-dialog.md#parameters)). A parameter row may carry `when` for this purpose only.

## Object rows

Every template object has its own UUID, local to the template. Creation mints fresh board UUIDs ([creation](creation.md)).

| Table | Row | Notes |
|---|---|---|
| `outline` | `{uuid, vertices, locked, when?}` | Exactly one row must survive every choice combination. |
| `cutouts` | `{uuid, vertices, locked, when?}` | |
| `keepouts` | `{uuid, layer, restrict, shape, protects?, locked, when?}` | |
| `parts` | `PartEntryRow` | The same as a sheet's part table: element UUID plus `hash`, with **no Library reference**. |
| `footprint_copies` | `FootprintCopyRow` | The same as a sheet's. A connector names the copy it uses, which is its Footprint choice. |
| `connectors` | `{uuid, part, footprint, at, rotation, side, locked, when?}` | |
| `holes` | Mechanical Part row, or Free Pad row | |
| `drawing_groups` | `{uuid, name, layer, when?}` | See [drawings](drawings.md#format). |
| `drawings` | `{uuid, group, kind, …, locked, when?}` | See [drawings](drawings.md#format). |

- **Contours.** The `outline` and `cutouts` rows use the Board Outline's own vertex list, where each stored vertex holds the edge that leaves it. Every number in it may be an expression.
  - A vertex may carry `plated: true` to mark the edge it starts as a Plated Edge. A template stores no Net ([plated edges](plated-edges.md#board-templates)).
  - Zero-length edges, such as a corner radius that evaluates to 0, are dropped at creation.
- **Keepouts.**
  - `layer` is symbolic: `all_copper`, `top_copper` or `bottom_copper`. There are no inner-layer references in v0.2.5.
  - `restrict` is the v0.1 Keepout set (`tracks`, `vias`, `zones`, `smd_pads`, `tht_pads`, `footprints`).
  - `protects: <uuid>` names a connector or hole. When that object is excluded or gated out, the Keepout goes with it. Keepouts that name nothing always come along.
- **Connectors.**
  - `part` is a `parts` entry UUID, and `footprint` is a `footprint_copies` UUID.
  - `at` and `rotation` are each an integer or an expression. `side` is `top` or `bottom`.
  - There is no Designator: creation gives the board's next free number.
  - Any Part can be a connector row. "Connector" names the typical use, not a rule.
- **Holes.**
  - A Mechanical Part row has the same shape as a connector row. It is placed without Sync and gets no Symbol ([creation](creation.md#holes)).
  - A Free Pad row holds the whole Pad model in board coordinates.
- **Coordinates** are copied 1:1. Built-ins put the lower-left corner of the outline's bounding box at (0,0). Templates never set the Output Origin.
- **Parts are found by the normal registered-Library lookup**, as Source Missing and Extract to Library already do. Creation never fails because a source is missing ([creation](creation.md#parts-and-missing-sources)).

## Validation

### When the file loads

These checks run on every load. A template that fails any of them is listed in the scan's `problems` and can't be picked.

- the schema and `deny_unknown_fields`;
- expression syntax, and that every name resolves;
- that every option of a choice binds the same names, and that `default` names an option;
- that every `when` names a choice parameter and its keys;
- that every `part`, `footprint`, `protects`, drawing `group` and Dimension `follow` reference resolves;
- that exactly one `outline` row survives every choice combination;
- that `suggested_layers` is in range.

### After evaluation

Evaluation takes a set of values and produces a frame. That frame goes through the normal Board Outline, Board Cutout and Keepout checks.

- An invalid frame blocks creation, and the error is shown under the preview with its location marked ([dialog](creation-dialog.md#invalid-frames)).
- Preview and creation share one evaluation function ([commands](commands.md#libraryboard_templatepreview)).
