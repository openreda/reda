# Foxtrot trial (issue #442)

Throwaway harness for `../SPEC.md`. Only `read` applies: Foxtrot has no STEP writer
(`write` prints `{"ok":false,...}`).

## Versions

- Foxtrot: https://github.com/Formlabs/foxtrot at `69fb37688892246e670fc08eb60b7d3a92d70d3f`
  (2021-08-13, last commit upstream; not on crates.io). Crates used: `step`, `triangulate`
  (direct), `cdt`, `nurbs` (transitive). Default features, so single-threaded (no `rayon`).
- rustc 1.96.1. `Cargo.lock` here is the resolution cargo made on 2026-11-25; upstream ships no lockfile.
- Notable resolved versions: nom 6.1.2, bitvec 0.19.6, funty 1.1.0, nalgebra 0.27.1,
  nalgebra-glm 0.13.0, simba 0.5.1. 39 crates.io packages in the lockfile, plus the four Foxtrot crates.

## Licence

- Repo root has `LICENSE-MIT` and `LICENSE-APACHE` (Copyright 2021 Formlabs); the README says
  "Licensed under either of Apache-2.0 or MIT at your option".
- Only `cdt/Cargo.toml` has a `license` field (`MIT OR Apache-2.0`). `step`, `triangulate`,
  `nurbs` and `express` have none, so `cargo metadata` reports no licence for them; the
  repo-level statement is the only grant.
- `step/src/ap214.rs` is generated from the ISO schema file `10303-214e3-aim-long.exp`, which is
  not in the repo.
- Third-party crates: all MIT, Apache-2.0, MIT/Apache-2.0, BSD-3-Clause (nalgebra, nalgebra-glm),
  or Unlicense/MIT (memchr). No copyleft.

## Building on Rust 1.96

- Builds unmodified as git dependencies. No patches, no vendoring, no edition changes
  (the crates are edition 2018).
- Clean release build: 22 s wall on 16 cores. The generated AP214 parser (`step`, 39,611 lines)
  takes 14.5 s of that; nalgebra 4.0 s; `triangulate` 2.6 s.
- cargo warns that nom 6.1.2 and nalgebra 0.27.1 "contain code that will be rejected by a future
  version of Rust" (56 instances of `semicolon_in_expressions_from_macros`). Both are held back
  by Foxtrot's version requirements (`nom = "6.0"`, `nalgebra-glm = "0.13.0"`).
- `cargo check --release --target x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin` pass.

## What the reader could not do

- No tolerance setting. Curves and surfaces are sampled at fixed counts.
- Colour is looked up per solid only (`STYLED_ITEM` whose item is the `MANIFOLD_SOLID_BREP`).
  Per-face styles, which the KiCad and Würth files use, are ignored: those meshes come out all
  fallback grey (0.5, 0.5, 0.5). The fallback is indistinguishable from a real grey in the mesh.
- No assembly or product API. `triangulate()` applies the placements itself and returns one
  merged mesh; `products` is a count of `PRODUCT` entities taken from the entity list.
- No unit handling; coordinates are in file units. The harness adds a note when the file has no
  `SI_UNIT(.MILLI.,.METRE.)`.
- The parser is typed against AP214 only. Entities it does not know (the AP203 file has 10
  administrative ones, first `DESIGN_CONTEXT`) are logged and dropped; geometry was unaffected there.
- `BREP_WITH_VOIDS`: only the outer shell is meshed (`TODO: handle voids` in the source).
- Closed or self-intersecting B-spline surfaces and curves are rejected (error variants in `triangulate::Error`).

## API friction

- `StepFile::parse` returns no error; failures go to `log::warn!`. `triangulate()` returns
  `(Mesh, Stats)`; `Stats` has `num_errors` and `num_panics` (panics inside the CDT are caught by
  Foxtrot itself with `catch_unwind`), but faces rejected earlier for an unsupported surface or
  curve are only logged with `log::error!`. The harness installs a `log` logger and counts those
  messages by prefix to fill `faces_failed`.
- No face-to-triangle mapping, so `faces_meshed` is `num_faces - failures`; a face that yields
  zero triangles without an error is not detected.
- The code has many `expect`/`panic!`/`unwrap` calls outside the caught region (missing entity
  references, unknown representation types, unterminated input in `into_blocks`). They abort the
  process; the harness does not catch them.

## Results on the five sample files

All five exit 0 with nothing on stderr; no panics or hangs. All five meshes are closed
(`open_edges` 0) with positive volume. Times are single runs, single-threaded.

| file | faces meshed / total | triangles | parse ms | tess ms | colours | products |
|---|---|---|---|---|---|---|
| kicad SOIC-8 | 147 / 147 | 2490 | 1.9 | 3.0 | 0 (147 face styles ignored) | 1 |
| kicad R_0603 | 26 / 26 | 532 | 0.7 | 0.7 | 0 (26 face styles ignored) | 1 |
| wurth WE-PMI-0805 (AP242) | 26 / 26 | 1196 | 0.6 | 0.8 | 0 (26 face styles ignored) | 2 |
| ap203 470531000 | 101 / 101 | 588 | 1.6 | 1.0 | 0 | 1 |
| adafruit 3660 BME680 | 610 / 610 | 17452 | 11.3 | 25.4 | 10, 22 of 22 solids styled | 11 |

The BME680 bounding box is 25.4 x 17.78 x 4.53 mm from the origin, so the nested placements
were applied.
