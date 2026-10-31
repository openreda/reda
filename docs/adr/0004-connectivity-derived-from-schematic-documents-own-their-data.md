# Schematic connectivity is derived from the drawing; each Document owns its own copy of Component data

Nets on Schematic Sheets are not stored: the engine derives them live from the drawn Wires, Net Labels, Ports and Power Ports, using fixed scope rules. Component data (Designator, value, fields, footprint choice, Component Type) is the schematic's, duplicated on every Unit of the Component. The PCB Document keeps its own copy: Footprint instances linked to their Component by UUID, and stored PCB Nets (UUID, name, Net Class) that its copper references. Sync reconciles the two sides as a reviewable change list, matching Nets by Pin membership before name. We chose this because it is the model users of KiCad and of commercial integrated suites already know, and it keeps each Document self-contained enough to render and produce outputs on its own.

## Considered Options

- **An explicit Project-level netlist and Component table that Schematic Sheets and the PCB both view** (the Horizon EDA / LibrePCB model, recommended by our research): rejected. It makes sync trivial and command-level edits like "connect this Pin to GND" direct, but departs from the workflow our audience expects and makes every connect/disconnect a merge/split of stored Nets.

## Consequences

- A command that changes connectivity does so by editing the drawing (adding a Wire, a Power Port...), never by editing a Net directly.
- Schematic and PCB can disagree between syncs, so sync and its change list are core features, not conveniences.
- Copies of Component data on several Units can drift apart; commands keep them equal and ERC reports a mismatch.
- Undo is per Document; a cross-Document command records an independent entry in each Document it touches.
