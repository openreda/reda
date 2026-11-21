# v0.2.5 Template drawings

Source: [Are mechanical-layer drawings part of Board Templates?](https://github.com/openreda/reda/issues/404), with the drawing-group decision in [scope](scope.md#decisions-taken-while-writing-this-spec).

Board Templates carry drawings: the mating board's outline, a bracket, the key Dimensions. This file says what they are made of, how the format stores them, how the dialog and Save treat them, and what the built-ins carry.

## What a drawing is

- **Existing objects only:** graphics (line, arc, rectangle), text and Dimensions. There are no new drawing kinds.
- **Two Layers:** a drawing goes on the board's single Mechanical Layer or its single Documentation Layer.
  - Both Layers take graphics today (`takes_graphics`).
  - Both are named symbolically, as `"mechanical"` and `"documentation"`.
  - No fab output plots either of them.
- Nothing goes on silkscreen or copper.
- The v0.3 Layer `purpose` field may reshape this later. Breaking changes are allowed.

## Format

- **Drawing objects are template objects** like any other. Expression-valued geometry and `when` gates apply to them ([format](template-format.md)).
- **Drawing groups.** Each drawing belongs to exactly one named drawing group, a row `{uuid, name, layer, when?}`.
  - `layer` is `mechanical` or `documentation`.
  - Examples: "Host board outline (approximate)", "Key dimensions".
  - A drawing row is `{uuid, group, kind, …geometry, when?}`, where `kind` is `line`, `arc`, `rectangle`, `text` or `dimension`.
- **Dimension ends.** A template Dimension end is either a fixed point or follows a template object by its template-local id: `{follow: {object, vertex?}}`.
  - It may follow an outline vertex (the contour UUID plus a vertex index), a cutout, a hole, a connector origin or a Keepout.
  - At creation the id is remapped to the created board UUID.
- **Group UUIDs.** A drawing group's UUID is what `exclude` names ([commands](commands.md#exclude)). "Drawing group" is a format detail, not a glossary term.

## In the creation dialog

- **Ticks:** each drawing group is one tick in Includes, ticked by default ([dialog](creation-dialog.md#includes)).
- **Dropping with a target:** a Dimension whose followed object is unticked or gated out is left out with it, as a tied Keepout goes with its hole or connector.
- **Locked:** drawings start Locked, like every other created object.
- **Preview:** the 2D preview draws them muted, next to its own computed key dimensions. Unticking a group hides it.

## Save Board as Template

- **Two ticks,** "Mechanical drawings" and "Documentation drawings", both on by default. Locked status is not required.
- **Groups:** each tick becomes one drawing group named after its Layer.
- **Dimensions:** a Dimension keeps its reference when the target is captured. Otherwise its end becomes free.

See [save-as-template](save-as-template.md#drawings).

## Built-in drawings

### Layer convention

- **Documentation** holds the host or mating context.
- **Mechanical** holds the key Dimensions.

### Key dimensions

Every built-in except Custom rectangle has a **Key dimensions** group on the Mechanical Layer. It holds:

- the board width and height, driven by expressions;
- the hole pitch;
- the connector datum, where there is one: GPIO pin 1, the PCIe key, the M.2 notch.

Each template's set is listed in the [catalogue](catalogue.md). The M.2 group also holds the labelled far-end 3.80 zone.

### Host drawings

Two built-ins carry an approximate host drawing on the Documentation Layer. Everything else has none, because its outline is the host's (Eurocard, Uno, Mega, Feather, Pico, M.2) or the host context is already a pad.

- **Add-on board for Raspberry Pi**, group "Host board outline (approximate)":
  - the 85 × 56 Pi outline with its four holes;
  - the USB/Ethernet end marked;
  - the numbers in the [catalogue](catalogue.md#add-on-board-for-raspberry-pi).
- **PCI Express add-in card**, group "Bracket outline (approximate)": the bracket for the chosen height.
  - **Verified:** the bracket lengths, 120.0 (full height) and 79.2 (low profile).
  - **Single-source placement** ([card-edge research](https://github.com/openreda/reda/blob/research/card-edge-open-hardware/docs/research/card-edge-open-hardware.md#bracket)):
    - the tip sits 15.14 (full) or 11.11 (low profile) below the finger edge;
    - the inner face sits 1.03 outboard of the bracket-side card edge, at X = −1.03.
  - **Drawn as:**
    - the 0.86-thick sheet seen edge-on, from X = −1.89 to −1.03;
    - its top fold, 11.43 (full) or 11.84 (low profile), drawn outward.
  - The decision called this group "Host board outline (approximate)" for both templates. It is named "Bracket outline (approximate)" here because a bracket is not a host board.

These drawings are a deliberate exception to "ship verified numbers only":

- They sit on a Layer that no fab output plots, so they can't reach the board.
- They are marked "(approximate)" in the group name.
- Each template's `description` names the source.

Everything that reaches fab stays verified-only.
