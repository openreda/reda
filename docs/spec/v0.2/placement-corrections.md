# v0.2 Placement Corrections

Source: [How are CPL rotation corrections stored and applied?](https://github.com/openreda/reda/issues/338). It builds on the v0.1 [CPL](../v0.1/outputs.md#cpl) and [Fab Presets](../v0.1/outputs.md#fab-presets), and on the research into [CPL rotation corrections in other tools and at fabs](https://github.com/openreda/reda/issues/326).

This file says what a Placement Correction is, where it is stored, what it matches, how it changes the CPL, what ships, and how the user checks and adds one. The Command ids and the schema are decided in their own work on the [v0.2 map](https://github.com/openreda/reda/issues/319).

A v0.1 Project needs no change: every preset starts with an empty list, and an empty list writes the v0.1 CPL.

## Why they exist

A CPL angle only means something when the designer and the assembler share a zero. A fab's zero follows how the part sits in its tape, so it can differ from the Footprint's zero by package and even by part number. No fab publishes its zeros, and REDA cannot work them out. The user learns each one from the fab's preview and records it once.

## What one is

A **Placement Correction** is one row of a Fab Preset. It adjusts the rotation, and optionally the centre, that the CPL writes for the Components it matches.

| Field | Meaning |
|---|---|
| match | a Footprint name pattern, or a Component field and its value ([below](#matching)) |
| `rotation` | degrees counter-clockwise, added to the written rotation |
| `offset` | optional X and Y in mm, in the Footprint's own frame |

- A row with rotation 0 and no offset is legal. Placed above a wildcard row, it exempts one Footprint from that row.
- A correction changes only the CPL. The board, the Footprint, the Gerber files and the BOM are untouched.

## Where they live

- The Fab Preset holds an ordered `placement_corrections` list. There is no table in the Project, in the Library or on a Component.
- The list travels as the rest of the preset does: in a `.redafab` file, in the Project's copy, and to the next Project by "Save as user preset".
- **No per-Component override.** The same Part at the same fab always needs the same correction, so a one-off fix is a field row for that Part's supplier number. A per-Component value would have to be stored per fab on every Component and kept equal by Sync.
- The plug-ins researched do keep per-part fields (`JLCPCB_CORRECTION` and the like). REDA differs because its Fab Preset is already the place for what one fab expects.

## Matching

Two kinds of row:

- **Footprint row:** tests the Footprint's name alone, without its Library, ignoring case. `*` is the only wildcard, as in [Library search](../v0.1/library.md). A pattern without one must match the whole name. Example: `SOT-23*`.
- **Field row:** names any Component field and matches its whole value after trimming, ignoring case. An empty value never matches. Example: `LCSC` = `C25804`.

Which row applies to a Component:

1. the first field row that matches, in list order
2. otherwise the first Footprint row that matches, in list order
3. otherwise none

- Only one row applies. Rows never add up.
- A row that matches nothing on this board is silent, because a preset serves many boards.
- There are no regular expressions.

## What a correction changes

- **Rotation:** the correction is added after the preset's [`bottom_rotation`](../v0.1/outputs.md#cpl) formula, and the sum is brought into 0 to 360.
  - top: written = θ + d
  - bottom: written = bottom(θ) + d
  - This is the order the two verified community tools use (mirror first, then add), so a number learned from them or from the fab's preview means the same here.
- **Offset:** it moves the `centre` position only. `origin` and `pad1` are literal points that a fab asks for as such.
  - It is given in the Footprint's own frame, before rotation and side. On the bottom it is mirrored with the Footprint.
  - `bottom_position` and `file_origin` then apply to the moved centre as they do to any centre.
- The hand [acceptance check](../v0.1/quality-and-release.md#testing) covers the bottom-side order, as it covers the bottom keys.

## What ships

- Generic, JLCPCB, PCBWay and every other shipped preset start with an empty list.
- No community table is shipped. None is clearly shippable under [ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md): the known ones are GPL, AGPL or of unverified provenance.
- There is no importer for the community CSV formats in v0.2. Their rows are regular expressions over KiCad footprint names, so they would match nothing in a REDA Library. An importer can come back with full KiCad import (v0.6), when those names exist in REDA.
- Users share corrections by sharing a `.redafab` file.

## The Placement page

Placement Corrections have their own [Fab Preset page](../v0.1/outputs.md#fab-preset-pages), **Placement**, beside CPL.

| Page | Holds |
|---|---|
| Placement | the `placement_corrections` grid: match kind, pattern or field and value, rotation, offset X, offset Y, with up, down, add and remove |

They are the one part of a preset the user builds up build by build, so two actions treat them with care:

- **Reset page to source** on the CPL page leaves them alone, because they are on another page. The Placement page has its own.
- **Update from source** merges the list instead of replacing it:
  - A row is identified by its match (kind, pattern, or field and value).
  - Source rows are added, or update the Project's row with the same match.
  - Project rows the source lacks are kept, in their order, ahead of new source rows. The preview lists them as kept.
- **Save as user preset** carries every row.

## Checking and adding

REDA cannot know how a fab's part sits in its tape, so it draws no corrected placement. The fab's own preview stays the check, and REDA keeps the loop from that preview back to the preset short.

- The Output Job window's [CPL preview](../v0.1/outputs.md#output-job-window) gains read-only columns: stored rotation, correction, written rotation, and the row that matched. The offset shows with the correction.
- **Add correction…** on a preview row opens a small form, pre-filled with that Component's Footprint name, or with a supplier field when the preset has a required one. It writes to the Project's copy of the preset, and the preview updates.
- A row whose match already exists opens that row for editing instead of adding a second.
- The Output Manifest needs nothing new: the Project's copy of the preset already decides the run.

## Left to other work on the map

- Command ids and the `.redafab` schema for `placement_corrections`.
- Goldens and read-back tests for corrected CPL files, top and bottom.
