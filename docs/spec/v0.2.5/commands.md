# v0.2.5 Commands

Source: [Command and MCP surface for Board Templates](https://github.com/openreda/reda/issues/401), with [Save Board as Template](https://github.com/openreda/reda/issues/402), [plated edges](https://github.com/openreda/reda/issues/403), [drawings](https://github.com/openreda/reda/issues/404) and the drawing-group decision in [scope](scope.md#decisions-taken-while-writing-this-spec).

This file lists every new or changed Command in v0.2.5. They follow ADR [0001](../../adr/0001-every-action-is-a-command.md) and ADR [0007](../../adr/0007-generic-mcp-tools-and-always-bridge.md). None is added to the dedicated MCP `QUERY_TOOLS`; agents reach them through `run_command`. The Command coverage test covers them as it stands.

## Creating a board

### `board_template`

`project.new` and `document.add {kind: pcb}` both take `board_template?: BoardTemplateChoice`, one shared struct:

```
BoardTemplateChoice {
  element: Uuid,                 // the template's Library element UUID
  values?: {name: value},        // choice: option key; length: Length
  exclude?: [Uuid],              // template object UUIDs to leave out
}
```

- **Without `board_template`, the board has no outline.** That is the dialog's "No outline (draw it later)".
- `{template}` stays free for the v0.7 sheet frames.
- **Identity.** A template is named by its Library element UUID only, as every other element is. There is no name selector; agents get UUIDs from `library.board_template.list`.

### `values`

- A choice takes its option `key` (`"x4"`). A length takes a `Length`, either integer nm or a string such as `"65mm"`.
- An omitted parameter takes its default. A length uses its declared default. A choice uses its `default`, or else its first option.
- **Errors:** an unknown name, an unknown option key, or a length out of range. The range is evaluated at the chosen options, since `min` and `max` may name bound names ([format](template-format.md#parameter-kinds)).
- A value for a parameter hidden by `when` is accepted and ignored.

### `exclude`

- `exclude` may name these template object UUIDs:
  - connector rows;
  - hole rows;
  - drawing groups.
- Leaving it out means everything is ticked.
- **Errors:** naming the outline, a cutout, a Keepout, a single drawing, or any UUID that isn't one of the kinds above.
- **What drops with it:**
  - A Keepout whose `protects` names an excluded object drops with it.
  - A Dimension that follows an excluded object drops with it.
  - Excluding a drawing group drops every drawing in it.
- A row that a `when` gate removes is accepted and ignored, so the dialog can keep its ticks while options change.

### Layer count

When `board_template` is given and `layers` is omitted, the template's resolved `suggested_layers` is used. An explicit `layers` always wins. With no template, the default stays 2. `construction` and `fab_preset` are unchanged from [v0.2](../v0.2/commands.md#creating-a-board).

### Failure

Creation is all-or-nothing. An invalid evaluated frame, or bad `values` or `exclude` input, fails `project.new` or `document.add` with nothing written. The error is `invalid_params` and carries the same `problems` list the preview returns.

### Reporting

`seeded` gains `template: {element, name, values, objects}`:

- `values` is fully resolved, with the defaults filled in. A parameter hidden by its `when` is left out: it gates and binds at its default, but it is not a value of this board.
- `objects` holds one entry per created template object: `{template_object, created, designator?, component?}`.
  - It covers the outline, cutouts, Keepouts, holes, connectors, drawing groups and drawings. A drawing group's `created` is absent, because it creates no board object of its own.
  - `designator` is given for connectors and Mechanical Part holes, and `component` for connectors.
- Excluded and gated-out objects are absent.
- `seeded.outline` stays.

### Library visibility

- List and preview scan everything registered: the open Project's Libraries if there is one, plus user and Starter.
- Each entry carries its Library's `origin`: `starter`, `user` or `project`.
- **New Project** hides `project`-origin entries, and `project.new` refuses a template from a Project Library ("not available to a new Project").
- **Add PCB Document** shows all origins.

## What is removed

- **`project.new {board}` is removed.** Custom rectangle is a Starter template ([catalogue](catalogue.md#custom-rectangle)).
- **The dialog's *Draw later / Rectangle* radio** is replaced by the Browser picker ([dialog](creation-dialog.md)).
- **Old Journals** that used `{board}` are not migrated. `new_project.jsonl` is rewritten ([testing](testing.md#creation-path)).

## Library Commands

`list` and `preview` work with no Project open, so New Project can call them. `save` needs an active PCB Document.

### `library.board_template.list`

`library.board_template.list {library?}` is a Query. For each template it returns:

```
{element, name, description, category, library, origin, suggested_layers,
 parameters: [{name, kind, label, options | default/min/max/ranges/unit, when?}],
 objects: [{uuid, kind, part?, side?, protects?, group?}]}
```

- A length's `min` and `max` are its range at the default choices, in nm.
- `ranges: [{when, min, max}]` is its range under each option of the choices its bounds name, such as `{when: {lanes: ["x16"]}, min: 129300000, max: 167650000}` for PCIe `card_length`. The dialog shows the one whose `when` holds for the options chosen. A range that names no choice is one entry with an empty `when` ([#416 decision](https://github.com/openreda/reda/issues/416#issuecomment-7072198747)).

- `kind` is `outline`, `cutout`, `keepout`, `connector`, `hole`, `drawing_group` or `drawing`. A drawing names its `group`.
- The result also holds the scan's `problems`, including templates that failed their [load checks](template-format.md#when-the-file-loads).
- No generic `library.element.list` is added.

### `library.board_template.preview`

`library.board_template.preview {BoardTemplateChoice}` is a Query. It returns the evaluated frame:

- the outline and cutout vertices, with their `plated` marks;
- the Keepouts;
- the holes and connectors as placed, in integer nm, with pad outlines for drawing;
- the drawings, by group;
- the resolved `values` (hidden parameters left out, as in [reporting](#reporting)) and `suggested_layers`;
- `problems`: Board Outline, Board Cutout and Keepout validation, with locations so the preview can mark them.

The dialog's egui painter draws from this output, and creation runs the same evaluation function.

### `library.board_template.save`

`library.board_template.save {library, name, description?, category?, objects?}` acts on the active PCB Document. It returns the new element UUID, its path and any warnings. See [save-as-template](save-as-template.md). The UI reaches it through **Design ▸ Save Board as Template…**.

## Board Commands

- **`pcb.contour.edge.set {contour, edge, plated: {net?} | null}`** marks or clears a Plated Edge. See [plated edges](plated-edges.md#marking).
  - `contour` is the Board Outline's or a Board Cutout's UUID.
  - `edge` is the index of the vertex that starts the edge.
- **`pcb.contour.edge.set_all {contour, plated: {net?} | null}`** marks or clears every edge of one contour in one undoable edit: the "Plate all edges" and "Unplate all" shortcuts. See [plated edges](plated-edges.md#marking).
- **`pcb.edge_connector.set {edge_connector: {bevel?: {angle, depth}, hard_gold?} | null}`** sets or clears the board's `edge_connector` property. It is one undoable edit. No Command sets board-level properties today (finish lives on the Stackup and goes through `pcb.stackup.set`), so this one is new. See [edge connectors](edge-connectors.md#the-edge_connector-board-property).
- **The Pad `role: edge_connector` and `castellated` fields** are set through the Footprint editor's Pad Commands and `pcb.pad.*` like any other Pad field.
