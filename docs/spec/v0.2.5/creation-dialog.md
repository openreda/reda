# v0.2.5 Creation dialog

Source: [Prototype the Board Template picker and preview in the creation dialog](https://github.com/openreda/reda/issues/398), Variant A (Browser), from the [prototype](https://github.com/openreda/reda/tree/prototype/board-template-picker). Extended by [drawings](https://github.com/openreda/reda/issues/404) and the [command surface](https://github.com/openreda/reda/issues/401).

This file says how New Project and Add PCB Document present Board Templates. The dialog replaces the *Draw later / Rectangle* radio of the [v0.1 New Project dialog](../v0.1/app.md#new-project). The layer count, Construction and Fab Preset fields keep their [v0.2 behaviour](../v0.2/layer-count.md#choosing-the-count-for-a-new-board).

## Layout

It is one page, with no steps.

- **Top row (New Project only):** Name, Location and Browse.
- **Left pane:**
  - a search field over the template list;
  - the list, grouped by `category`. Generic comes first and the rest are alphabetical. User templates with no category go under "Other".
  - The list shows every template in every scanned Library, from `library.board_template.list`.
  - Search filters by name, description and category.
- **Right pane, top to bottom:**
  - the template's name and `description`;
  - the large [preview](#preview) and its legend;
  - the [parameters](#parameters), beside the [Includes](#includes) list;
  - Fab Preset, Copper Layers and Construction, under the parameters.

## Default

- **"No outline (draw it later)"** is preselected and shown bold, as today's dialog does.
- It is a Generic entry, and Custom rectangle sits next to it.
- With it picked, the dialog sends no `board_template`, and the preview shows an empty state.

## Parameters

- A choice with 2–3 options is a segmented control, and one with more is a dropdown.
- A length is a number field in mm, with its min–max range beside it. The range is the one for the options chosen, from `library.board_template.list`'s `ranges`, so PCIe x16 shows 129.30–167.65 ([commands](commands.md#libraryboard_templatelist)).
- A parameter whose `when` does not hold is hidden, such as the cutouts on the micro Pi board.
- Changing a parameter keeps the Includes ticks. A tick on a row that is now gated out is kept and ignored.

## Includes

- There is **one tick row per Part**, such as "4× MountingHole_M3 + Keepouts" or "J1 Socket_2x20 (bottom)".
- **A group with several items expands**, so each item can still be ticked on its own. Each item is one connector or hole row.
- **Each drawing group is one tick row**, such as "Key dimensions" or "Host board outline (approximate)". A drawing group doesn't expand.
- Everything is ticked by default.
- **Unticking:**
  - an unticked connector or hole is drawn faint in the preview;
  - its tied Keepouts and following Dimensions disappear;
  - an unticked drawing group is hidden.
- Unticked rows are sent as `exclude`.

## Preview

The preview draws the output of `library.board_template.preview` and updates on every change. It draws:

- the board fill and outline, cutouts, holes and Keepouts;
- Plated Edges, drawn thicker in the top copper Layer's colour;
- connector bodies with their pads, coloured by side: top solid, bottom dashed;
- the template's drawings, muted;
- the overall width and height, plus the template's key datum where it has one, such as PCIe's 57.15 from key to bracket. These are the preview's own computed dimensions, drawn on top of the template's drawings.

The legend names the colours: board, cutout, hole, Keepout, top, bottom and drawings.

### Painter

The preview is a simple egui painter, as prototyped, not the reda-render canvas. A static 2D top view with labels needs no camera, Layers or GPU path. The same painter can draw small thumbnails if they are ever wanted. It goes through theme tokens and REDA widgets, as ADR [0015](../../adr/0015-chrome-styling-goes-through-theme-tokens-and-reda-widgets.md) requires.

### Invalid frames

- The preview still draws the frame, with each problem's location marked.
- The validation error shows under the preview.
- **Create** (or **Add**) is disabled until the frame is valid.

## Layers note

- Under the layer count, a note reads "The template suggests N copper layers".
- The template's suggestion fills the layer count until the user types their own. After that, changing the template doesn't overwrite it.
- When the field holds the suggestion, the dialog omits `layers`, so the Command takes the suggestion itself.

## Add PCB Document

Add PCB Document uses the same dialog without the Name/Location row.

- Its Fab Preset dropdown keeps the project-copy entries it has today.
- Its list also shows the Project's Libraries.

## What Create sends

Create sends exactly one `project.new` or `document.add`, with:

- `board_template {element, values, exclude}`, where `values` holds every visible parameter and `exclude` every unticked row;
- or no `board_template` when "No outline" is picked.
