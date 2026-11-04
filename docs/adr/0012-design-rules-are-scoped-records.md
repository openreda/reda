# Design rules are scoped records from v0.1

REDA stores every design rule, board-wide and per Net Class, as a record in one table with a kind, a scope and typed values, instead of a fixed board-wide struct with override fields on Net Class. v0.1 only accepts board-wide and Net Class scopes, but one store gives "which rule applies here?" a single lookup, and later per-Layer and per-area rules become new scope filters rather than a second mechanism.

## Considered Options

- **Keep the fixed struct as the fallback and add scoped overrides later:** adding them would be additive, but rule lookup would have two paths for good, and giving Net Classes min and preferred values would reshape them anyway.

## Consequences

- The file must hold exactly one undeletable board-wide record per kind, checked on load.
- Fab Presets keep flat minimums, because a fab's limits have no scope.
- Scope filters on PCB objects must use their own names: `objects.query`'s `net` means a schematic Net.
