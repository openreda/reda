# v0.3 Mechanical Layers and Purpose

Source: [What does a Layer's purpose mean, and which purposes exist?](https://github.com/openreda/reda/issues/435), after the survey in [How do other EDA tools give mechanical and documentation layers a purpose?](https://github.com/openreda/reda/issues/429).

This file says what a [Mechanical Layer](../../../GLOSSARY.md) is from v0.3 on, what its [Purpose](../../../GLOSSARY.md) is and what it drives, and what changes for new boards and Board Templates. The Commands and file fields are decided in [What Commands and file-format additions does the v0.3 data need?](https://github.com/openreda/reda/issues/440). How DXF import and export use the Purpose is decided in [How do DXF import and export work?](https://github.com/openreda/reda/issues/436).

## What changes from v0.2.5

- Up to v0.2.5 a board has exactly one Mechanical Layer and one Documentation Layer, unnamed and unsided, told apart only by a [template convention](../v0.2.5/drawings.md#layer-convention).
- From v0.3 there is **one kind, the Mechanical Layer**, and a board holds any number of them. The Documentation Layer kind is removed.
- Each Mechanical Layer has a **name**, a **Purpose** and an optional **side**.
- This breaks earlier board and Board Template files. That is allowed; there is no migration duty.

## The kinds that stay

- Courtyard, Assembly and the Board Outline stay Layer kinds. They carry behaviour (checks, outputs, the outline), always exist, and have no Purpose.
- Purpose applies to Mechanical Layers only.

## A Mechanical Layer

- **User-owned.** The user adds, removes and renames Mechanical Layers. There is no cap on the count, and a board may have none.
- **Name.** Required, and unique among the board's Mechanical Layers. A new Layer takes its Purpose as its name, numbered on a clash ("Dimensions 2"). Renaming never changes the Purpose.
- **Board-only.** Footprints do not draw on Mechanical Layers. Their non-fab shapes stay on Courtyard and Assembly.
- **Contents.** Graphics, text and Dimensions, as today.
- **Removing** one deletes its objects. A preview lists them first, as removing a copper Layer does.

### Side

- A Mechanical Layer's side is none (the default), top or bottom.
- It is a tag. There is no pairing: no Layer is the partner of another, and nothing moves between Mechanical Layers when a Component flips.
- A sided Mechanical Layer:
  - is shown by the Top or Bottom Layer Set of its side;
  - when bottom, is drawn as other bottom Layers are, so its text reads correctly from the bottom;
  - lets side-aware defaults and later drawing outputs pick it by side.
- Pairing is revisited only if a Footprint-level Purpose (glue points, component centre) is wanted.

## Purpose

- A value from a closed list. Free text belongs in the name.

| Purpose | Holds |
|---|---|
| General | anything; the default for a new Layer |
| Dimensions | Dimensions and their notes |
| Fabrication Notes | notes and drawings for the board maker |
| Assembly Notes | notes and drawings for the assembler |
| Reference | things that are not this board: a host or mating board, an enclosure, a bracket |

- **Extending the list.** A Purpose is added when something reads it. Left out for now: V-cut and route tool path (with Panels), sheet or title block (no board drawing output yet), and glue, coating and gold plating (labels in every tool surveyed).
- **Several Layers may share a Purpose.** Wherever a default needs one Layer, the first with that Purpose in Layer order wins.

### What a Purpose drives

Defaults and matching only:

- The Dimension Tool defaults to the first Dimensions Layer.
- DXF import and export pre-fill their Layer mapping by Purpose.
- Board Templates and Import part find their target Layer by Purpose.

What it does not drive:

- **No check.** A Purpose restricts nothing. Any graphic, text or Dimension may sit on any Mechanical Layer, and a Dimension on a Reference Layer raises nothing.
- **No 3D.** No Mechanical Layer is drawn in the [3D view](3d-view.md) or written to STEP, whatever its Purpose.
- **No fab plot.** No fab output plots a Mechanical Layer.
- **No visibility default.** All Purposes are shown by default.

The Purpose can be changed at any time. The Layer's objects stay as they are.

## A new board

- A new board starts with two Mechanical Layers, both unsided:
  - **Dimensions**, with the Purpose Dimensions;
  - **Reference**, with the Purpose Reference.
- These are the former Mechanical and Documentation Layers under their real jobs.

## Board Templates

Changes to the v0.2.5 [drawings](../v0.2.5/drawings.md):

- **A drawing group names its Layer by Purpose**, not as `mechanical` or `documentation`.
- **Creation** puts the group on the board's first Mechanical Layer with that Purpose, and adds one if there is none.
- **Built-ins:** Key dimensions groups use `dimensions`, and host or mating context uses `reference`.
- **Save Board as Template** offers one tick per Mechanical Layer that has content, instead of the fixed two. Each ticked Layer becomes one drawing group named after the Layer, carrying its Purpose.
- Nothing else in the drawings spec changes.
