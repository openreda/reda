# v0.3 Component height

Source: [How does the Component height rule work?](https://github.com/openreda/reda/issues/459), in scope by [Which adjacent work ships in 0.3.0, and is it one release or two?](https://github.com/openreda/reda/issues/439).

DRC checks how tall Components are in two ways. A board-wide Rule sets a maximum for each side, and a Keepout can limit height inside its area. Both read the Footprint's stored `height`, which [Bodies](bodies.md#height) keeps authoritative, not the Bodies. Commands and file-format details are in [Commands and file formats](commands.md).

## What is measured

- A Component's height is its Footprint's `height`. A Component has no height of its own.
- Height is measured outward from the board surface on the side the Component is placed: up from the top surface for a top-side Component, down from the bottom surface for a bottom-side one.
- **Not Fitted Components are skipped** by both checks, because a part that isn't mounted can't be too tall. Variants will widen this to "fitted in any Variant".

## The `component_height` rule

| Kind | Values | Check | Default |
|---|---|---|---|
| `component_height` | `top_max?`, `bottom_max?` | a Component's height above the maximum for its side | both empty |

- **Board-wide only.** Any non-empty `scope` is refused. A single Component that is allowed to be taller, such as one large electrolytic, is handled with a [Waiver](../v0.1/checks.md#waivers), which records the reason.
- **Empty means no limit.** A new board has the board-wide record with both values empty, so nothing is checked until the user sets a value.
- **Fab Presets don't seed it.** Height comes from the enclosure, not the fab.
- **Validation:** a value that isn't positive is blocking ([validation](../v0.1/checks.md#validation)).
- **Board Setup** gets a Component height page, with Top max and Bottom max fields and the check's severity beside them.

## Height Areas

A [Keepout](../v0.1/pcb.md#keepouts) gains an optional `max_height`. A Keepout that has one is a **Height Area**.

- **Only with `restrict: footprints`.** On a Keepout without it, `max_height` is refused.
- **Meaning:** without `max_height`, a footprints Keepout still means no Footprint may be there. With it, no Footprint taller than `max_height` may be there.
- **Checked like the footprints restriction:** by Courtyard, or the [fallback box](../v0.1/checks.md#courtyards), with no added clearance, against Footprints on the Keepout's side. A Keepout on all Layers applies to both sides.
- **Validation:** `max_height` must be positive. Zero is refused, because a plain footprints Keepout already means "none at all".
- **Editing:** the Keepout's Properties and the Keepout Tool options gain a Max height field, enabled when Footprints is ticked. There is no separate Tool mode. `pcb.keepout.add` and `pcb.keepout.set` take `max_height`, and null clears it.
- **Board Templates** carry `max_height` with their Keepouts, including in templates the user saves. The built-in M.2 and PCI Express templates get no height limits, because those figures sit behind the paywalled M.2 and PCI-SIG specifications ([catalogue](../v0.2.5/catalogue.md)).

## Checks

| Check | Fires on | Severity |
|---|---|---|
| `component_height` | a Component taller than the board-wide maximum for its side, or than the `max_height` of a Height Area its Courtyard overlaps | E |
| `height_unknown` | a Component whose Footprint has no `height` while a limit applies to it (the board-wide maximum for its side, or a Height Area it overlaps) | W |

- **One Violation per limit broken.** A Component over the board-wide maximum and over two Height Areas gives three Violations.
- **Violation fields** ([Violations](../v0.1/checks.md#violations)): `measured` is the height, `limit` the maximum that applied, and `rule` the board-wide record's UUID. A Height Area Violation has no `rule` and instead names the Keepout in `args.keepout`, and its objects are the Component and the Keepout.
- `height_unknown` is not the stand-in's 1 mm. The Stand-in Body is a display fallback and never passes a check.
- **Overlay** is on by default for `component_height`, colouring the whole Component.
- **Live during a move:** a Courtyard over a Height Area it is too tall for shows in the violation colour, like the [live overlap](../v0.1/pcb.md#moving-footprints) of Courtyards. The board-wide maximum isn't shown live, because moving a Component doesn't change it.
- **Explaining:** `pcb.rules.applicable` and the Properties Rules section show, for a Component, the board-wide maximum for its side and every Height Area it overlaps, with the one it breaks marked.

## In 3D

- **Height Areas:** a Keepout with `max_height` is drawn as a translucent prism from the board surface on its side out to `max_height`. It is a "Height Areas" kind in the [3D view](3d-view.md) chip's Show menu, off by default.
- Plain Keepouts are not drawn in 3D.
- STEP export leaves Height Areas out, and a Board Image includes them only when they are in the Show set it is given.

## Out

- A `min` or `preferred` height, and scopes by Designator or Footprint.
- Height checks against the Bodies' geometry, and Body-against-Body collision ([out of scope](https://github.com/openreda/reda/issues/439)).
- Height limits in the built-in M.2 and PCI Express templates.
