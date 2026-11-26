# STEP is read and written by REDA's own code, with no CAD kernel

REDA reads STEP models and writes STEP board exports with its own code in a `reda-step` crate, and takes no CAD kernel as a dependency. One typed model of Part 21 records serves both directions: the reader parses a model into it and tessellates the faces for the 3D view, and the writer builds the board as exact B-rep geometry and writes every embedded model back out from its parsed records, converted to millimetres and to the export's protocol. The only third-party crate is `spade` for constrained Delaunay triangulation. solidcore and SolidCraft (tangency-labs, Apache-2.0 and MIT OR Apache-2.0) are references to read, not code to copy.

The trial on 58 real models showed writing needs no kernel and reading is the hard part: OpenCascade drew 54 at the right volume, solidcore 51 with every colour, and truck and Foxtrot 35 to 38. OpenCascade is LGPL-2.1, which ADR-0002 excludes and ADR-0017 makes stricter, because one LGPL dependency would rule out the MIT version.

## Considered Options

- **Bind OpenCascade:** the best reader, used by KiCad, Horizon EDA and LibrePCB, but LGPL.
- **Depend on solidcore:** the best permissive reader, but its STEP and tessellation crates are unpublished, change daily and come with a 1.35M-line kernel port.
- **Fork solidcore's read subset (about 50k lines):** a working reader on day one, but the copied files stay Apache-2.0 under someone else's copyright, and REDA would maintain code it did not design.
- **truck or Foxtrot:** pure Rust, but no or partial colours, a third of models wrong, and crashes.
- **An in-house writer that copies each model's text unchanged:** the smallest route the trial proved, but it converts no units or protocol and cannot read, so view and export would not share one model.

## Consequences

- REDA builds and owns the geometry: the Part 21 parser and writer, analytic and B-spline surfaces, trimmed faces, the trimming curve fitter, and the tessellator. That is a large build, and the reader is tested against a model corpus with OpenCascade's 54 of 58 as the target.
- The reader draws what it can instead of refusing: an open shell is meshed open, a degenerate loop is dropped, and the seam cases solidcore refuses are handled. A model that cannot be drawn shows as a stand-in Body with a warning; it is still exported if it parsed.
- Models are STEP only. Meshes (WRL, OBJ, glTF, STL) cannot feed a STEP export, which KiCad 10 gave as its reason for dropping its WRL models.
- The reader is in every build, with no Cargo feature to leave it out, and adds no C++ toolchain or large binary.
- Any piece later ported from solidcore or SolidCraft keeps its Apache-2.0 header and NOTICE, and cannot be relicensed to MIT.
