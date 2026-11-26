# v0.3 testing

Source: [Which tests does v0.3 add?](https://github.com/openreda/reda/issues/441). It builds on the [v0.1 testing](../v0.1/quality-and-release.md#testing), the [v0.2 testing](../v0.2/testing.md) and the [v0.2.5 testing](../v0.2.5/testing.md), and on every other file in this folder, each of which left its tests to this one.

This file says which reference Projects, goldens, read-back tests, render checks, scenarios, property tests, UI tests, fuzz targets and hand checks v0.3 adds. Everything earlier testing specs set and this file does not name is unchanged: the kinds of test, the deterministic engine, `reda-testkit`, the Command coverage test and the merge gate.

- **Automated:** everything except the [model corpus](#model-corpus) and the [hand checks](#hand-acceptance-checks) runs in `cargo test`.
- **CI** gains one scheduled job, the weekly model corpus run. Like `fuzz.yml`, it never gates a merge.
- **Outside tools stay out of CI.** The v0.1 rule "no GPL tools, even in CI" covers LGPL CAD applications too, so FreeCAD, BRL-CAD and LibreCAD are used only by hand. Automated read-back uses REDA's own readers.

## Reference Projects

One is added under `tests/projects/`, built by its own script in `tests/scenarios/` and kept as files, with the full golden set (ERC, DRC, Sync and every output).

| Project | What it carries |
|---|---|
| `mechanical_3d` | four [Mechanical Layers](mechanical-layers.md) with the Purposes General, Dimensions, Fabrication Notes and Reference, one of them sided bottom; generated Extruded Bodies (SOIC, chip, QFN, DIP, header); a hand-drawn Footprint with a From Courtyard Body; Model Bodies from the [STEP fixtures](#step-fixtures), one of which parses but draws nothing and gives a Stand-in Body; Components on both sides; a Not Fitted Component; Board Cutouts, slots, plated and non-plated holes; a [Height Area](component-height.md#height-areas) and a `component_height` rule with one Waiver; a Default Board Grid, a Cartesian and a polar Local Grid, and Guides; an Output Job with `step`, `dxf` and `board_images` on |

- The existing reference Projects get no Bodies.
- `usb_c_mcu` also gets a STEP golden, because it has Stand-in Bodies on both sides.
- `mechanical_3d` is DRC-clean apart from the waived Component. The new checks fail on purpose [in the scenarios](#checks-that-must-fire).

## Migration

The step from format 2 to 3 is in [commands](commands.md#format-version).

- **Frozen inputs,** exactly as `0.2.0` wrote them:
  - every reference Project: `minimal_board`, `blinky_555`, `usb_c_mcu`, `four_layer`, `six_layer_hdi`, `m2_card` and `castellated_module`;
  - one `.redaboard` with drawing groups on both the old Mechanical and Documentation layers;
  - one of each Library file kind;
  - one `.redafab` and one `.redarules`.
- **Goldens:** the migrated files.
- **Unchanged outputs:** every fab output of each migrated board (Gerber, drill, BOM, CPL, job file, Fab Notes) is byte-identical to what `0.2.0` wrote. Mechanical Layers are not plotted, so any difference is a bug.

## Output goldens

Byte-exact goldens under the v0.1 rules (fixed `Clock` and REDA version):

- the `.step` export of `mechanical_3d` and `usb_c_mcu`;
- the `.dxf` export of `mechanical_3d`;
- the Output Manifest of the `mechanical_3d` job, with the STEP, DXF and Board Image entries.

Board Images are [exempt from determinism](board-image.md#determinism) and get no byte golden. A test checks that each ticked view's PNG exists with the expected pixel size and that the manifest lists its hash.

## STEP

### STEP fixtures

A small set of committed, licence-clean STEP files under `crates/reda-step/tests/fixtures/`, with a README giving each file's origin. Each covers one case:

- every surface and curve type the reader evaluates;
- each [tolerant reading](step.md#tolerant-reading) case: an open shell, a loop enclosing no area, a band face with several loops around its axis, an open surface of revolution with one bound, and a solid that still fails;
- AP203, AP214 and AP242 headers;
- inch units;
- per-face and per-solid colours;
- a nested assembly;
- a `.stpz` file.

They come from two sources: files REDA's own writer writes, and models the maintainer draws in FreeCAD or Onshape and commits under MIT. Output from an LGPL program is not bound by its licence. No vendor, distributor or library Model is committed ([ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md)).

### Model corpus

The reader is measured against the [trial's](https://github.com/openreda/reda/issues/442) 58 real models, which cannot be committed.

- **Manifest:** a committed `step-corpus.tsv` with one row per model: name, source URL, pinned commit or SHA-256, schema, and FreeCAD's reference volume and bounding box.
- **Test:** an `#[ignore]` test reads the folder named by `REDA_STEP_CORPUS` and reports, per model, whether every face meshed, the volume is within 1 % of the reference, the mesh is closed, and the colours were read, plus any crash or timeout. It grades as the trial did.
- **Fetch script:** downloads the 40 openly fetchable models (KiCad's library and Würth's KiCad library, at pinned commits). The other 18 stay on the maintainer's disk.
- **Weekly job:** fetches the 40 and runs the test. It never gates a merge.
- **Target:** OpenCascade's result, 54 of 58 at the right volume. It is reported, not a gate, and falling short does not block 0.3.0.

### Read-back

`reda-step` reads each STEP export back and compares it with the model:

- the board solid's volume from the Board Outline, Board Cutouts, cut holes and Stackup thickness;
- one sub-assembly per exported Component, named by Designator with the name prefix;
- each Body's placement, with bottom-side Bodies rotated and never mirrored;
- colours from the Stackup and Materials;
- Stand-in Bodies where the options say.

Opening exports in FreeCAD and BRL-CAD is a [hand check](#hand-acceptance-checks). Commercial MCAD tools are optional and tracked in [Check REDA's STEP export in commercial MCAD tools](https://github.com/openreda/reda/issues/452).

## DXF

### DXF fixtures

Committed under `crates/reda-import-dxf/tests/fixtures/`, from two sources:

- **Hand-written** from Autodesk's DXF Reference: each read entity, binary DXF, R12 `POLYLINE`, Windows-1252 text, nested `INSERT`, a flipped extrusion, and a file with no `$INSUNITS`.
- **Saved by the maintainer** from real tools (FreeCAD, LibreCAD, Inkscape, Fusion), drawing their own shapes, and committed under MIT.

Each piece ported from cadcraft ([DXF](dxf.md#own-reader-and-writer)) has unit tests against these fixtures and the DXF Reference.

### Read-back

A DXF export imported back lands on the same Layers at the same place, by the same-name pre-fill rule. Opening exports in outside tools is a [hand check](#hand-acceptance-checks).

## EasyEDA fixtures

Committed under `crates/reda-import-easyeda/tests/fixtures/`, from two sources:

- **Hand-written** from EasyEDA's published specifications: Std, Pro v2 and Pro v3.
- **Saved by the maintainer** from the EasyEDA editor, for parts of their own design.

Downloaded LCSC parts and vendor zips never go into the repo. They are a [hand check](#hand-acceptance-checks).

## Rendering

- **3D Scene text snapshots,** on every platform. Each mesh is serialised by its source (board, copper, silkscreen, Body, Stand-in Body, Height Area), transform, colour and triangle count, not by vertices. They cover `mechanical_3d` from the top and the bottom, the Show set with kinds off and at opacity, Height Areas on, and a highlight.
- **PNG snapshots,** Linux only, through Mesa's software renderer with a per-pixel tolerance: `document.render` with `mode: "3d"` at three-quarter, bottom, perspective, and a transparent background.

## Scenarios

- **One data-driven `.jsonl` scenario per area,** beside the one that builds `mechanical_3d`:

  | Scenario | Area |
  |---|---|
  | `mechanical_layers` | [mechanical-layers](mechanical-layers.md): add, set, move and remove with the preview; first-with-Purpose defaults such as the Dimension Tool's Layer; a Board Template drawing group found by Purpose |
  | `bodies` | [bodies](bodies.md): add, edit, remove and reorder; From Courtyard; Seat on board; Centre on pads; Set height from Bodies; regenerating a generated Footprint replaces its Bodies |
  | `models` | [models](models.md): Add, Replace and Save Model; Models shared by hash; an unused Model deleted on save; placing a Footprint copies its Models into the Project |
  | `import_part_models` | [Import part](models.md#import-part): a `.kicad_mod` with models, a vendor zip and a folder (built from REDA-written fixtures), KiCad path variables, the `.wrl` to `.step` swap, an unreferenced STEP, a scale applied |
  | `easyeda_import` | [easyeda-import](easyeda-import.md): Std, Pro v2 and Pro v3; `devices` in a multi-device file; the field mapping; the Courtyard; each `height` source; a matched, a picked and a missing Model |
  | `step_export` | [step-export](step-export.md): every option of `pcb.export_step` and the problems list |
  | `dxf` | [dxf](dxf.md): preview and import with each pre-fill rule, Board Outline replacement, gaps marked; export with each `entries` kind and in inch |
  | `board_image` | [board-image](board-image.md): the 3D parameters of `document.render`, the camera returned, and the `board_images` Output Job |
  | `component_height` | [component-height](component-height.md): the rule, Height Areas, a Waiver, `pcb.rules.applicable` |
  | `grids_guides` | [grids-guides](grids-guides.md): Board Grid, Local Grid and Guide Commands, reorder, Align with `key`, Rotate Components to grid, Move Board Shape carrying Local Grids and Guides, carrying them in a Board Template and Save as Template's tick rows |
  | `input` | [input-and-defaults](input-and-defaults.md): `view.cursor.step` with X ≠ Y, inside a polar Local Grid and from off the grid; `view.cursor.click`; the three new nudges |
  | `object_defaults` | [Object Defaults](input-and-defaults.md#object-defaults): reset, save, and load with skipped entries counted |
| `model_picks` | [Placing a Model by picking](bodies.md#placing-a-model-by-picking): `face_on_board` on a planar face and on a circular edge, `move_point_to` onto a Pad centre, and both on a mesh-only Model, each one undo step returning the placement |

- **Refusals are Rust scenarios,** each writing nothing:
  - `purpose` on a Layer that isn't Mechanical, and a duplicate Mechanical Layer name;
  - `split` with `kind: mechanical`;
  - a Model file that isn't STEP Part 21;
  - a `max_height` that isn't positive, and `max_height` on a Keepout that doesn't restrict footprints;
  - a `component_height` record with a `scope`;
  - `mode: "3d"` on a Schematic Sheet, and each 2D-only parameter in 3D, with the error naming every one given;
  - a `size` outside 1 to 8192;
  - a STEP export with a missing or an open Board Outline;
  - a DWG file given to DXF import;
  - a second `default` Board Grid, and a duplicate Local Grid name.
- **Command coverage:** the v0.1 test is unchanged, so every new Command, the app-registered 3D view Commands included, must be run by a test.

### Checks that must fire

| Check | Case | Scenario |
|---|---|---|
| `component_height` | over the board-wide maximum and inside two Height Areas, giving three Violations | `component_height` |
| `height_unknown` | no `height` where a limit applies | `component_height` |
| `model_unreadable` | a Model that parses but draws nothing | `bodies` |
| `body_height_mismatch` | Bodies topping out above `height` | `bodies` |

**Must not fire:** `component_height` on a too-tall Not Fitted Component, and on a Component with no `height` (the Stand-in Body's 1 mm is never taken as a height).

## Property tests

`proptest` gains:

- **Snap order:** for random Guides, Local Grids and Hotspots, the snap point equals a naive reference following Hotspot, Guide, Alignment Guide, Local Grid, grid, with Local Grids ranked by list order.
- **Arrow steps:** from any point, N presses land on grid lines, and N presses one way then N back return to the first landed point.
- **DXF round trip:** a random Board Outline with Board Cutouts and Mechanical graphics, exported then imported, lands within 1 nm per vertex, and arcs stay arcs.
- **Spline and ellipse fit:** random splines and ellipses are fitted within 0.005 mm at dense samples, and a circular ellipse gives one true arc.
- **STEP round trip:** a random board and set of Extruded Bodies, written then read by `reda-step`, keeps its volume within 0.1 %, its names and its colours.
- **EasyEDA placement:** for a random transform and model bounding box, the folded offset and rotation put the model's vertices where EasyEDA's formula does, within 1 nm. A size ratio more than 2 % from 1 is reported.
- **Bottom side:** a bottom-side Body's transform in STEP has determinant +1, so it is rotated and never mirrored.
- **Identity and migration:** save → load → save and the 2 to 3 step cover every new field. Undoing any sequence of Body, grid or Guide edits restores the saved bytes.

## UI tests

`egui_kittest`, asserting on the Journal as in v0.1:

- **3D view:**
  - the 2D | 3D switch and the `3` and `2` keys give `view.3d` and `view.2d`;
  - a click on a Body selects its Component, as in 2D;
  - a left drag turns the view and adds no edit Command;
  - Measure with two clicks shows the length with its X, Y and Z parts, and the readout gives the height above board before the first click;
  - Measure Clearance on two Bodies gives `view.3d.measure_clearance` and shows the shortest span;
  - Shift+`1` to Shift+`7` give `view.3d.look`, a Tool key gives `view.2d` then starts the Tool, and an editing key gives no Command and the status-bar message;
  - New view of this Document gives `view.new_document_view`; a selection or Layer hidden in one view shows in the other, and each keeps its own mode;
  - a board saved in 3D reopens in 3D from `.reda/view.json`;
  - Save image… gives one `view.3d.save_image` with the on-screen camera;
  - the 3D view Preferences page writes `pcb.view3d.*`.
- **Footprint editor:** dropping a `.step` file gives exactly one `library.footprint.body.add_model`, and a file over 20 MB adds a warning; Seat on board gives one `library.footprint.body.seat_on_board`; Face on board and Point to point each give one Command and Esc gives none.
- **DXF import dialog:** changing a mapping row and then Import gives one `pcb.import_dxf` with that `mapping`.
- **Mouse Wheel page:** a clashing combination is refused, naming the action that holds it; choosing a preset rewrites the table.
- **Keys:**
  - arrows give `view.cursor.step`, and `Mod`+arrows give `pcb.nudge`;
  - Enter gives `view.cursor.click` in a Tool that doesn't bind Enter;
  - Enter ends a route, and Shift+Enter places a route corner;
  - with a non-US layout simulated, the top-row key in the `1` position fires a binding on `1`, with and without Shift; keypad `3` gives `view.3d` in the 3D view and `pcb.layer.activate` in 2D;
  - G cycles the board's In-cycle Board Grids together with the user's presets, and a board opens on its Default grid.
- **Object Defaults:** with Remember last used on, a Tool-option edit writes `defaults.*`; with it off, it doesn't.
- **Height Areas:** during a move into a Height Area, a too-tall Component shows `component_height` live, and the 3D view's Height Areas switch draws the prism.
- **Mechanical Layers:** a bottom-sided Mechanical Layer is shown by the Bottom Layer Set and hidden by the Top one.
- **KiCad 3D models folder:** with a fake KiCad install on the path, the Preference fills in by auto-detection.
- **PNG snapshots,** Linux only: the 3D view with its chip and orientation gizmo, the DXF import dialog with a gap marked, and the Mouse Wheel page.

## Fuzzing

The weekly fuzz job gains three targets: the DXF reader, the STEP reader and the EasyEDA reader. It never gates a merge.

## Performance

There are still no gates. `#[ignore]` timing tests cover reading the largest corpus model, the STEP export of `mechanical_3d`, and building the 3D scene for the [32-Layer board](../v0.2/testing.md#the-32-layer-limit). They report against the 3D view's [frame-time targets](3d-view.md#frame-time).

## Hand acceptance checks

Maintainer checks. They never gate a 0.x release, and a failure reopens the ticket that made the decision.

- **STEP:** the `mechanical_3d` and `usb_c_mcu` exports open in FreeCAD and BRL-CAD as valid solids with the right names, placements and colours. Commercial MCAD tools are optional ([#452](https://github.com/openreda/reda/issues/452)).
- **DXF:** the `mechanical_3d` export opens in FreeCAD, LibreCAD and `ezdxf audit`, and in Onshape or Fusion where available. The closed `LWPOLYLINE` outline extrudes directly.
- **3D view by hand:** the mouse and touchpad feel, on real hardware. The prototype was judged from screenshots only.
- **Arrow keys move the pointer** on Windows, macOS, X11 and Wayland, including the disabled Preference where the desktop refuses.
- **Real downloads:** an LCSC part in each EasyEDA family, and a SnapMagic, an Ultra Librarian and a SamacSys zip, import with their Models placed correctly.
- **KiCad rotation order:** a KiCad footprint whose model has a non-trivial `(rotate (xyz …))` looks the same in REDA's 3D view as in KiCad's.
