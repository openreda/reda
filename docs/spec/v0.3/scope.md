# v0.3 scope

Source: [Which adjacent work ships in 0.3.0, and is it one release or two?](https://github.com/openreda/reda/issues/439).

This file says what ships in `0.3.0` besides the Mechanical & 3D milestone itself, what was left out, and whether the canvas extras get their own release.

## One release

- **0.3.0 holds both halves:** the Mechanical & 3D milestone and the canvas extras moved from v0.2 (document grids and guides, wheel mapping, the arrow-key cursor, Object Defaults).
- There is no separate canvas release, as Board Templates had with 0.2.5. Nobody uses REDA yet and 0.x releases are dev releases, so an earlier release would gain little.
- The canvas extras come early in the build order, because the 3D view uses the same wheel mapping. The order itself is settled with the spec layout on the [v0.3 map](https://github.com/openreda/reda/issues/421).

## In 0.3.0

- **A rendered 3D board image.** `document.render` gains a 3D view, drawn headless from the same `reda-3d` scene as the 3D view, so CLI and MCP agents can see the board in 3D. An Output Job entry writes board images beside the zip. The 2D `document.render` from v0.1 is unchanged. Details: [What does a rendered 3D board image contain, and how is it asked for?](https://github.com/openreda/reda/issues/458).
- **A Component height rule.** A new Rule kind checks each Component's `height` against a maximum. Details: [How does the Component height rule work?](https://github.com/openreda/reda/issues/459).

## Left out

| Item | Why |
|---|---|
| Body collision checks (3D Body against Body) | they need solid or mesh intersection on imported Models, which `reda-step` does not have at first; `courtyard_overlap` already catches most clashes in 2D. Later work, together with checking fit against an enclosure Model |
| DXF into the Footprint editor, and DXF on copper | already left for a later milestone by [How do DXF import and export work?](https://github.com/openreda/reda/issues/436) |
