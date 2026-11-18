# Design rules are scoped records from v0.1

REDA stores every design rule, board-wide and per Net Class, as a record in one table with a kind, a scope and typed values, instead of a fixed board-wide struct with override fields on Net Class. v0.1 only accepts board-wide and Net Class scopes, but one store gives "which rule applies here?" a single lookup, and later per-Layer and per-area rules become new scope filters rather than a second mechanism.

## Considered Options

- **Keep the fixed struct as the fallback and add scoped overrides later:** adding them would be additive, but rule lookup would have two paths for good, and giving Net Classes min and preferred values would reshape them anyway.

## Consequences

- The file must hold exactly one undeletable board-wide record per kind, checked on load.
- A Net can be in several Net Classes. Records have no priority field: the winner comes from a rank on the Net Class, one per class rather than one per rule kind, until the query language brings typed priorities.
- Fab Presets keep flat limits, because a fab's limits have no scope.
  - Amended for v0.2: a Fab Preset may also hold one `inner` block of limits, because some fabs publish different inner-Layer figures. It is the only scope a preset has ([layer-rules](../spec/v0.2/layer-rules.md#fab-presets)).
- v0.2 adds the `layers` and `net_class_pair` filters with a fixed order (pair, then Net Class, then Layer; a named Layer before a group) and still no priority field.
- Scope filters on PCB objects must use their own names: `objects.query`'s `net` means a schematic Net.
