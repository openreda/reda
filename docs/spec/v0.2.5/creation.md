# v0.2.5 Creating a board from a template

Source: [How do a template's mounting holes and connectors enter the design?](https://github.com/openreda/reda/issues/395), with the [command surface](https://github.com/openreda/reda/issues/401), [drawings](https://github.com/openreda/reda/issues/404) and [plated edges](https://github.com/openreda/reda/issues/403).

This file says what `project.new` and `document.add` write when they are given a `board_template`. The Command parameters are in [commands](commands.md), and the dialog is in [creation-dialog](creation-dialog.md).

## The rule

A template is only a shortcut to a board you could build by hand. Afterwards the board holds plain, editable integer objects and keeps no link to the template. Templates apply only at creation; applying one to an existing board is [out of scope](scope.md#out-of-scope).

## Steps

1. **Evaluate** the template with the given `values`, filling in defaults, through the same function `library.board_template.preview` uses ([commands](commands.md#libraryboard_templatepreview)).
2. **Drop** the gated-out rows and the `exclude`d rows, plus anything tied to them:
   - Keepouts whose `protects` names a dropped object;
   - Dimensions that follow a dropped object ([drawings](drawings.md#in-the-creation-dialog)).
3. **Validate** the frame ([format](template-format.md#after-evaluation)). If anything fails, nothing is written, and the error carries the preview's `problems` list.
4. **Write** the objects below, with fresh board UUIDs, as one Journal entry.

## What is written

### Outline and cutouts

- The Board Outline and Board Cutouts are written as ordinary contours, with the coordinates copied 1:1. Zero-length edges are dropped.
- The outline follows the normal Board Outline behaviour, including its Locked state.
- Edges marked `plated` become Plated Edges with no Net. They show the "no Net" warning until the user sets one ([plated edges](plated-edges.md#board-templates)).

### Connectors

Connectors become **normal Components, written on both sides at creation**.

- **The Symbol** goes on the first sheet, beside any existing content, with a normal Designator (J1, or the next free number).
- **The linked Footprint** is placed at the template's `at`, `rotation` and `side`, with the template's Footprint choice.
- **The result is the same state as placing, syncing and moving by hand**, so the next Change List is empty.
  - Nothing is left pending, and Sync is unchanged.
  - Sync gets no placement hints.
- A template never binds to an existing Component. The user unticks a connector they already have.
- There are no Net Labels or wire stubs on the connector Pins. That is schematic content, which belongs to project templates ([out of scope](scope.md#out-of-scope)).

### Holes

- **A Mechanical Part row** is placed directly, without Sync, as from the Library panel. It gets a Designator (H1, or the next free number) and no Symbol.
- **A Free Pad row** becomes a Free Pad.
- Neither carries a Net.

### Keepouts

- The symbolic `layer` is mapped onto the new board: `all_copper` to every copper Layer, `top_copper` to the top Layer and `bottom_copper` to the bottom Layer.
- `restrict` is copied.
- The `protects` tie only matters at creation. Afterwards the Keepout is a plain object.

### Drawings

Graphics, text and Dimensions go on the board's Mechanical or Documentation Layer, as their group says ([drawings](drawings.md)). A Dimension end that follows a template object gets that object's new board UUID.

### Board properties

A template's `edge_connector` is copied onto the new PCB Document ([edge connectors](edge-connectors.md#the-edge_connector-board-property)).

## Locked state

Connectors, holes, Keepouts and drawings all start Locked. Each row's `locked` field records this: built-ins set it true, and a saved template records each object's state when it was saved ([save](save-as-template.md#coordinates-and-locked-state)).

## Layer count

- When `board_template` is given and `layers` is omitted, the template's resolved `suggested_layers` is used. An option may override it.
- An explicit `layers` always wins.
- With no template, the default stays 2.
- The Fab Preset and Construction stay the user's choice. A template never sets them.

## Parts and missing sources

- **A template embeds full copies** of the Parts it uses, so creation never fails over a missing Part.
- **The created Component** carries that embedded copy. When no registered Library holds the source, it shows Source Missing, and Extract to Library recovers it.
- **Starter Parts.** The Parts that built-ins need are also in the Starter Library ([catalogue](catalogue.md#new-starter-parts)), so a board made from a built-in shows them Up to Date.

## Reporting

`seeded.template` maps each template object to what was created ([commands](commands.md#reporting)).
