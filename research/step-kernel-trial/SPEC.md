# STEP kernel trial: harness contract (issue #442)

Each candidate gets one standalone Cargo package in `research/step-kernel-trial/<candidate>/`.
Its `Cargo.toml` carries an empty `[workspace]` table so it is not part of the REDA workspace.
Pin every git dependency with `rev`. Build with `CARGO_TARGET_DIR=/data/mij/scratch/claude/reda-442/target/<candidate>`.

## `read <file.step>`

Reads one STEP file, tessellates it if the candidate can, prints exactly one JSON object on stdout
(one line), and exits 0 even when the file fails (put the message in `error`). A panic or hang is
caught by the runner (`timeout`), so do not wrap panics.

Fields (use `null` when the candidate cannot provide one):

- `parse_ok` bool, `parse_ms` number: Part 21 text to the candidate's own model.
- `error` string or null: first failure, shortened to 300 characters.
- `solids`, `shells`, `faces_total`: counts found in the file's B-rep.
- `faces_meshed`, `faces_failed`: faces that produced triangles, and faces that did not.
- `triangles`, `tess_ms`.
- `bbox_min`, `bbox_max`: `[x,y,z]` of the mesh, in millimetres, in the root assembly frame
  (apply assembly placements if the candidate exposes them; say in `notes` if it does not).
- `area`: sum of triangle areas, mm². `volume`: signed volume by the divergence theorem, mm³.
- `open_edges`: after welding vertices closer than 1e-4 mm, the number of mesh edges used by
  exactly one triangle (0 means a closed mesh).
- `colours_distinct`: distinct RGB colours the reader exposes. `faces_coloured`: faces (or
  solids, say which in `notes`) that have a colour.
- `products`: assembly nodes (products or instances) the reader exposes.
- `notes`: short free text.

Tessellation tolerance: chordal deviation 0.01 mm, or the nearest setting the candidate has.

## `write <out.step> <component.step>`

Writes an assembly named `trial-board` holding:

1. Product `PCB`: a prism, green (0.0, 0.5, 0.2). Outline: rectangle 0,0 to 50,30 mm with 3 mm
   rounded corners (four arcs), thickness 1.6 mm (z from 0 to 1.6). Holes through it: four round
   holes of 3.2 mm diameter centred 4 mm in from each corner, and one slot 2 mm wide and 8 mm long
   (centre to centre 6 mm) centred at (25, 15) along X. The solid must be one closed shell of
   planes and cylinders; cut the holes in 2D, not with 3D booleans, if the candidate allows.
2. Product `U1`: the given component model, placed at (20, 10, 1.6) rotated 90 degrees about +Z,
   keeping its own colours if the route can.
3. A second instance of the same component as `U2` at (35, 20, 1.6), no rotation, sharing the
   definition with `U1` if the route can.

Print one JSON line: `{"ok":bool,"error":..,"bytes":..,"schema":"..","notes":".."}`.
If a part of this cannot be done by the candidate (colours, assembly, re-embedding the component),
write what can be done and say exactly what is missing in `notes`.

## What to hand back

- The package builds in release mode with the pinned toolchain (`rust-toolchain.toml`, 1.96).
- `README.md` in the package: versions and commits used, what the candidate could not do, API
  friction, and anything surprising. Plain statements; no speculation.
- Do not run the whole model sample; the runner does that. Test on two or three files.
- Do not commit; do not touch files outside your package directory and your target directory.
