# Board Templates are parametric Library elements

A Board Template is a Library element (`board-templates/*.redaboard`, beside Symbols, Footprints and Parts) rather than a preset in its own config folder as Fab Presets are, so built-in templates ship in the Starter Library and user, team and per-project templates reuse Library registration, resolution order and sharing, with the Parts a template embeds sitting next to the Library they came from. A template declares choice and length parameters, and its numeric geometry fields may be expression strings over those parameters (evaluated exactly by the numeric-expression evaluator), a deliberate exception to ADR-0005's integers-only rule, because one file must cover a family such as PCIe x1–x16 or a free-sized rectangle, and the built-in templates must be written in the same format users write.

## Considered Options

- **A standalone folder like `<config>/fab-presets/`:** simpler, but it needs its own lookup and has no per-project or shared-library story.
- **A fixed variant table of literal frames:** keeps files integer-only, but can't express free lengths and duplicates near-identical frames.
- **Built-in templates as Rust generators, like the IPC footprint families:** rejected because users couldn't write the same thing.

## Consequences

- Structural variation (which Part, side, rotation, which objects exist) uses `when` gates on objects; choice options bind numeric values only, except for `suggested_layers`.
- Expressions in templates see only parameter names and option-bound values; there are no functions, conditionals or object-attribute references.
- A board created from a template holds only plain integer objects and keeps no link to the template.
