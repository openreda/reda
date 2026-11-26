# v0.3 STEP reading and writing

Source: [Does REDA take a CAD kernel dependency for STEP?](https://github.com/openreda/reda/issues/430), after [Can REDA read and write STEP without a CAD kernel, and what would a kernel cost?](https://github.com/openreda/reda/issues/422) ([findings](https://github.com/openreda/reda/blob/research/step-kernel-options/docs/research/step-kernel-options.md)) and [Trial the kernel-free STEP options against real models and MCAD importers](https://github.com/openreda/reda/issues/442) ([findings](https://github.com/openreda/reda/blob/research/step-kernel-trial/research/step-kernel-trial/FINDINGS.md)). Decision record: [ADR-0018](../../adr/0018-step-read-and-written-in-house.md).

This file says how REDA reads and writes STEP: the crate, the shared record model, what the reader tolerates, and what is out. What a board export contains (protocol, copper, naming) is in [STEP export](step-export.md), and what a Body is in [Bodies](bodies.md).

## No CAD kernel

- REDA takes no CAD kernel dependency. OpenCascade is LGPL-2.1 and excluded by [ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md) and [ADR-0017](../../adr/0017-gpl-public-releases-mit-contributions.md).
- **Own code:** the Part 21 parser and writer, the typed records, B-rep evaluation of analytic and B-spline surfaces, trimmed faces, the trimming curve fitter and the tessellator are all written in-house.
- **One dependency:** `spade` (MIT OR Apache-2.0) for constrained Delaunay triangulation of trimmed faces.
- **References to read:** [solidcore](https://github.com/tangency-labs/solidcore) (Apache-2.0) and [SolidCraft](https://github.com/tangency-labs/solidcraft) (MIT OR Apache-2.0). Worth reading:
  - the record layer (`solidcore-record/src/step/`): Part 21 parsing, product structure, colours and units;
  - `solidcore-step`'s topology and seam handling, and the self-contained trimming curve fitter in the kernel's `eval::pcurve` (SolidCraft's own trimming leans on the whole SMLib port and is not a model);
  - `solidcore-tess` for the face mesher;
  - their refusal classes, which the reader must handle instead (see below).
  - No code is copied. A piece that is ever ported keeps its Apache-2.0 header and the NOTICE, and stays under that licence.

## The `reda-step` crate

- A new crate, `reda-step`, holds everything STEP. It is used by `reda-3d` (the 3D scene) and `reda-output` (STEP export). Nothing else touches Part 21.
- **One typed model:** a STEP file is parsed into typed records (product structure, placements, shapes, styles, units), and the writer writes from the same records. Anything REDA can read, it can write.
- **Reading a model:** parse to records, convert to millimetres, then tessellate each face to a mesh with its colour, keeping the assembly tree and placements.
- **Writing a board:** the board is built by REDA from the Board Outline as exact B-rep geometry (lines, circles, planes, cylinders), with no 3D booleans. Each embedded model is written from its parsed records, converted to millimetres and to the export's protocol, not copied as text.
- The reader is in every build. There is no Cargo feature that leaves it out.

## Formats

- **STEP only:** `.step`, `.stp` and `.stpz` (gzip). AP203, AP214 and AP242 are read.
- **No meshes:** WRL, OBJ, glTF and STL are not read. They cannot feed a STEP export, and the 3D view would then differ from what is exported. KiCad 10 dropped its WRL models for this reason.

## Tolerant reading

The reader draws what it can rather than refusing a model:

- an open shell is meshed open;
- a loop that encloses no area is dropped;
- a band face with several loops around its axis is split along seams;
- an open surface of revolution with one bound gets a pole or seam;
- a solid that still fails is skipped and reported, and the rest of the model is drawn.

When nothing of a model can be drawn, the 3D view shows the [Stand-in Body](bodies.md#no-body-the-stand-in), the Footprint's body box raised to `height`, with a warning naming the file. A model that parsed is still exported even if it could not be tessellated. A model that fails to parse is left out of the export, with a warning, and the [STEP export](step-export.md#bodies) writes the Stand-in Body in its place when stand-ins are on.

## Tests

- A model corpus test measures the reader against the trial's 58 models (not committed, because of their licences) or a licence-clean equivalent. The target is OpenCascade's result: 54 at the right volume.
- Details are in [testing](testing.md).
- Checking an export in commercial MCAD tools (SolidWorks, Fusion, Onshape) is optional and tracked in its own issue. FreeCAD and BRL-CAD are the importers REDA can check itself.

## Out

- A CAD kernel dependency of any kind, and copying solidcore code.
- Mesh model formats.
- 3D booleans, enclosure fit checks and general 3D editing.
