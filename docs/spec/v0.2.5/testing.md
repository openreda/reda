# v0.2.5 Testing

Source: [How are Board Templates tested?](https://github.com/openreda/reda/issues/405). It follows the [v0.2 testing](../v0.2/testing.md) conventions.

- **Automated:** everything except the hand checks below runs in `cargo test`. CI gets no new job.
- **Acceptance checks** are maintainer hand checks. They never gate a 0.x release.

## Built-in catalogue

### Exhaustive data test

The test walks every choice combination of every built-in, with each length at its min, default and max as evaluated under that combination, since `min` and `max` may name bound names ([format](template-format.md#parameter-kinds)). For each point it checks that:

- the template loads and passes the schema and expression checks;
- exactly one Board Outline survives;
- the evaluated frame passes the outline, cutout and Keepout checks;
- each Part `hash` matches the Starter Library copy.

**Expected failures.** The test holds a list of the expected-invalid points, such as a Custom rectangle corner radius over half a side. It asserts that each one fails and nothing else does.

- **The list has no PCIe entry** ([#416 decision](https://github.com/openreda/reda/issues/416#issuecomment-7072198747)). A PCIe `card_length` shorter than the finger tab is not an invalid frame: the tab sticks out past the body and the outline stays simple.
- Instead, `card_length`'s `min` is the chosen lane width's tab end. The walk starts there, so x16 starts at 129.30. A shorter length is refused as out of range, which the `library.board_template.list` and preview tests cover.

### Frame goldens

- There is one golden per choice combination at default lengths, about 60 in all.
- Each is the `library.board_template.preview` result as canonical JSON.

### Board goldens

There are full PCB and Schematic goldens for one combination per template, from the `board_templates` scenario.

## Spec numbers

A checked-in `board-template-dimensions.tsv` has these columns: template, combination, quantity, expected value, tolerance and sources.

- **The data test** measures each quantity in the evaluated frame.
- **Sources:** a row needs two independent sources, or one primary spec marked `primary`. A single-source value that a ticket accepted knowingly is marked `decided` and names that ticket: the PCIe bevel (20°, 1.30), the PCIe pad near end (1.40), and the M.2 pad setback (0.50) and lands (Ø5.5 / Ø6.0) (all `decided #399`); the Feather hole drills (Ø2.7, the starter `MountingHole_M2.5`) and the Pico hole drills (Ø2.2, the starter `MountingHole_M2`) (`decided #397`).
- **Coverage:** every hole, notch, connector and outline dimension of each built-in at default values must have a row. Variants derived by expression are covered by the frame goldens.
- **Exempt:** Documentation drawings, such as "Host board outline (approximate)" and "Bracket outline (approximate)", are marked `unverified`.

## Creation path

### `board_templates` scenario

A `.jsonl` scenario runs `project.new` and `document.add` with `board_template`. It covers:

- `values` and `exclude`, where excluding a hole drops its `protects` Keepout and excluding a drawing group drops its drawings;
- Locked state;
- Symbols on the first sheet, with no Net Labels;
- `seeded.template`;
- an omitted `layers` taking the suggestion;
- `library.board_template.list`.

### Rust scenarios

- **Refusals**, each writing nothing:
  - an invalid frame;
  - an unknown template UUID;
  - an out-of-range length, including one below a range that depends on a choice (PCIe x16 under 129.30);
  - an unknown option;
  - an unknown `exclude` id;
  - an `exclude` naming a Keepout;
  - a Project-Library template given to `project.new`.
- **Source Missing.** A template whose Part no registered Library holds still creates the board, and the Component shows Source Missing. This is a success case, not a refusal ([creation](creation.md#parts-and-missing-sources)).

### Existing scenario

`new_project.jsonl` is rewritten from `{board}` to the Custom rectangle template. Old Journals are not migrated.

## Picker UI

- **Draw-list text snapshots** of the egui preview painter:
  - Pi full with cutouts;
  - Pi micro;
  - an invalid frame marked, such as Custom rectangle with a corner radius over half a side (no PCIe point is invalid since the [#416 decision](https://github.com/openreda/reda/issues/416#issuecomment-7072198747));
  - PCIe x16 showing its own `card_length` range, from 129.30;
  - one with an unticked connector drawn faint.
- **`egui_kittest` tests that assert on the Journal:**
  - changing a parameter and unticking an Include, then Create, gives exactly one `project.new` with the expected `board_template`;
  - an invalid frame disables Create;
  - a parameter hidden by `when` is not shown;
  - search filters the list.
- **One PNG snapshot** of the dialog, on Linux only.

## Save Board as Template

- **Round trip:**
  - Create a board from a built-in, save it as a template, then create from the saved template.
  - The two boards must be equal apart from UUIDs.
  - The saved frame must equal the built-in's at those values, as literals shifted to (0,0).
- **`proptest`:** a random frame survives save then create. The frame has an outline, cutouts, Keepouts, and Locked holes and Components.
- **Refusals:** a name clash, an inner-layer Keepout and a read-only Library.

## Outputs

There are two new reference Projects under `tests/projects/`. Each is built by a script in `tests/scenarios/` and gets the full golden set.

- **`m2_card`** is built from the M.2 template (key M, 2280). It carries Edge Connector Pads, the castellated half-moon and `edge_connector {bevel, hard_gold}`.
  - The M.2 template sets no bevel, so the script sets `bevel {20°, 1.30}` itself.
  - That value is a fixture, not a claim about M.2.
- **`castellated_module`** has Castellated Pads on two edges and a Plated Edge with a Net on a third. It also has a corner where two Plated Edges meet.

### Goldens

The goldens cover:

- `Profile,P`;
- `-plated_edges.gbr`;
- the job file keys `EdgeConnector`, `EdgeConnectorBevelled`, `HardGoldArea`, `EdgePlating` and `Castellated`;
- the Fab Notes.

### Read-back

- The Gerber reader checks the plated-edges file against the marked edges.
- The job-file reader checks the new keys.

### DRC cases

- **Must fire:** `edge_clearance` for copper of another Net beside a Plated Edge, and for a normal Pad at the outline.
- **Must not fire:** `edge_clearance` for an Edge Connector Pad.
- **Refused:** Plated Edges that meet with different Nets.

## Hand acceptance checks

These never gate a release. A failure reopens the ticket that made the decision.

- **Fab viewers:** load both new reference Projects in the JLCPCB and PCBWay viewers and in the reference Gerber viewer. Check that castellation and plated edges are recognised and the bevel note is read.
- **Fit check:** print each built-in's frame at 1:1 and lay the real host on it (Pi, Uno, Mega, Feather, Pico).
  - The print shows the outline, holes and connector pads, plus the Documentation drawing where there is one. Only the Pi has one among these hosts.
  - Hosts the maintainer doesn't own are noted as unchecked.
  - The ticket said to print the Documentation drawing. Only the Pi and PCIe have one, so the whole frame is printed instead.
- **Slot check:** once an M.2 or PCIe card has been fabbed, try it in a real slot.

## Unchanged

- The Command coverage test covers the new Commands as it stands.
- If v0.2.5 bumps `format` for the new Pad roles, Plated Edges or `.redaboard`, the v0.2 files are frozen and migrated with goldens, as v0.2 did.
