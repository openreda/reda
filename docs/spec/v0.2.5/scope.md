# v0.2.5 scope

Source: [Wayfinder: Board Templates for new boards (v0.2.5)](https://github.com/openreda/reda/issues/392).

v0.2.5 is one theme: **creating a PCB Document from a [Board Template](../../../GLOSSARY.md)**. A Board Template is a named, reusable starting point for a board's mechanical frame. It is picked with a live 2D preview in New Project and Add PCB Document. A template is only a shortcut to a board you could build by hand: afterwards the board holds plain, editable objects and keeps no link to the template.

The rest of this file says what ships, what each other file covers, and what was ruled out. When this spec and a ticket's resolution comment disagree, the ticket wins and this spec should be fixed.

## In 0.2.5

| File | What it holds |
|---|---|
| [`template-format.md`](template-format.md) | The `.redaboard` Library element: parameters, expressions, `when` gates, the object rows, where templates live and how they load. ADR [0016](../../adr/0016-board-templates-are-parametric-library-elements.md). |
| [`catalogue.md`](catalogue.md) | The built-in templates in the Starter Library, their numbers, and the new starter Parts. |
| [`creation.md`](creation.md) | What creating a board from a template writes: Components on both sides, Mechanical Parts, Keepouts, drawings, Locked state, layer count. |
| [`creation-dialog.md`](creation-dialog.md) | The Browser picker and live preview in New Project and Add PCB Document. |
| [`commands.md`](commands.md) | `board_template` on `project.new` and `document.add`, the `library.board_template.*` Commands, and what is removed. |
| [`save-as-template.md`](save-as-template.md) | Design ▸ Save Board as Template…: what it captures and how it writes the file. |
| [`edge-connectors.md`](edge-connectors.md) | Edge Connector Pads, Castellated Pads and the board's `edge_connector` property, with their fab outputs. |
| [`plated-edges.md`](plated-edges.md) | Plated Edges: marking, Nets, clearance, connectivity and fab outputs. |
| [`drawings.md`](drawings.md) | Mechanical and Documentation drawings carried by templates. |
| [`testing.md`](testing.md) | The automated suite and the hand acceptance checks. |

Edge Connector Pads, Castellated Pads and Plated Edges are general board features, not template features. They ship in v0.2.5 because the card-edge templates need the first two, and Plated Edges came out of the same decision ([card-edge fingers](https://github.com/openreda/reda/issues/399), [plated edges](https://github.com/openreda/reda/issues/403)).

## Build order

1. The format, its loader and its evaluator ([format](template-format.md)), with the exhaustive data test from [testing](testing.md#built-in-catalogue).
2. Edge Connector Pads and Castellated Pads ([edge connectors](edge-connectors.md)), because the card-edge Parts need them.
3. The new starter Parts and the built-in templates ([catalogue](catalogue.md)), with the dimensions table.
4. Creation and the Commands ([creation](creation.md), [commands](commands.md)).
5. The creation dialog ([dialog](creation-dialog.md)).
6. Save Board as Template ([save](save-as-template.md)).
7. Plated Edges ([plated edges](plated-edges.md)), which no other step depends on.

## Decisions taken while writing this spec

Four conflicts between the closed tickets came up while this spec was written. They were put to the maintainer and settled as follows. Each is recorded on the ticket it amends.

- **The M.2 half-moon is its own Mechanical Part**, not a Pad inside the card-edge Part. It sits at the card's far end, which moves with `length`, and a Part's Footprint can't follow a parameter. Amends [card-edge fingers](https://github.com/openreda/reda/issues/399).
- **The Feather template has no `format` choice.** Adafruit's mainboard and FeatherWing both plate the USB-end holes, and the starter library has no plated M2.5 Part, so the choice would change nothing. Amends [form factors](https://github.com/openreda/reda/issues/397).
- **Drawing groups have UUIDs, and `exclude` takes them**, so that unticking a drawing group in the dialog can be expressed as a Command. Amends the [command surface](https://github.com/openreda/reda/issues/401).
- **The M.2 far-end 3.80 zone is a drawing, not a Keepout.** The source allows components there, so any `restrict` set would forbid something the spec allows. Amends [form factors](https://github.com/openreda/reda/issues/397).

### Smaller calls made while writing

These fill gaps the tickets left. None changes a decision, and each can be overturned by editing this spec.

- **Ranges and defaults:**
  - Custom rectangle: width and height 1–1000, corner radius 0–500.
  - Pico `height` up to 150.
  - PCIe `card_length` from the lane width's tab end (the [#416 decision](https://github.com/openreda/reda/issues/416#issuecomment-7072198747) replaced a flat 70), with x1 and full height as defaults.
  - Eurocard defaults to 3U.
- **Format:**
  - Function calls are refused in expressions, as `expr.rs` already refuses them.
  - A bare number is read as mm or degrees.
  - `unit` is `mm`, `mil` or `in`.
  - `suggested_layers` is 2–32.
  - A parameter row may carry `when`, so it can be hidden.
- **Commands:**
  - `pcb.edge_connector.set` is the new Command for the board property.
  - Errors use `invalid_params`.
  - Drawing groups are listed in `seeded.template` with no `created`.
  - Save sends a drawing tick as the Layer's UUID.
- **Dialog:** the dialog omits `layers` while the field still holds the template's suggestion.
- **Parts and pads:**
  - The half-moon Part is named `MountingHole_M2_Castellated`, with the SATA-IO land sizes.
  - `castellated` can also be set on a Free Pad.
- **Built-ins:**
  - The Feather template's name.
  - The Pi Keepouts leave out `tht_pads`, so they don't flag their own hole.
  - The Pi host drawing is shown on the full size only.
- **Testing:**
  - The `m2_card` fixture bevel.
  - Decided single-source values are marked `decided` in the dimensions table.
  - The fit check prints the whole frame.

## Out of scope

These were ruled out on the [map](https://github.com/openreda/reda/issues/392). They are not fog, and they return only as a new effort.

- Project templates that carry schematic sheets, Net Labels, libraries or Output Jobs.
- Applying, re-applying or swapping a template on an existing board, and any link back to the template.
- A 3D preview in the creation dialog.
- Sheet frame templates (`project.new {template?}`), which stay in v0.7.
- Module-carrier footprints (Pico, XIAO, Nano, ESP32 DevKits). These are Parts on a free board, not Board Templates.
- Deferred from the built-in catalogue:
  - mikroBUS, because its notch and header datum are unverified;
  - Qwiic, because it has no standard;
  - PC/104, which is niche and needs new keyed connectors;
  - SparkFun MicroMod, which is not widely used.
- PCIe full length (312), the full-height upper bracket hole (94.75) and the Pi PoE-header keepout. Each has only one source or no dimension.
- A template editor. Templates are made with Save Board as Template, and parameters are added by hand-editing the file.
- Inner-layer Keepouts in templates.
