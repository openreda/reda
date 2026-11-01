# Parts bind a Symbol to alternate Footprints; Schematic Sheets carry the whole Part

A Library holds three kinds of element: Symbols, Footprints and Parts. A Part references one Symbol and an ordered list of alternate Footprints, and adds default fields and the Component Type. Pins map to Pads with the same number unless the Part records an override, such as a Pin mapped to several Pads, a Pin mapped to a different Pad, or a Pad left unconnected. When a Component is placed, its Schematic Sheet stores the whole Part: the Symbol, the overrides and a copy of every alternate Footprint. Sync can then add a Footprint, change it or work out the Pads' Nets with no Library present, as ADR-0005's "a Project opens and produces outputs on its own" requires. The PCB Document stores only the Footprints in use. We chose this so one Symbol and one Footprint can be reused across Parts whose pinouts differ (a SOT-23 transistor with BCE or EBC pins), and so a Footprint can be changed from either side without the Library.

## Considered Options

- **Symbol-centred parts** (Pin numbers are Pad numbers and the Symbol names its Footprint, as KiCad does): rejected. Each pinout needs its own Symbol or Footprint, and there is no place to map one Pin to several Pads without repeating Pad numbers. Importing such parts still works: the implicit Pin-number = Pad-number map covers them with no overrides.
- **Sheet stores only the Symbol, the PCB stores the Footprint** (as first sketched for the file format): rejected, because Sync would then need the Library to place or swap a Footprint.

## Consequences

- Each Schematic Sheet holds Footprint geometry for every alternate of each placed Part, deduplicated by content hash. This makes sheet files larger, in return for Projects that need no Library.
- The schematic shows the Pad number(s) that a Pin is mapped to on the Component's current Footprint, not the Symbol's own Pin number, so pin labels can change when the Footprint changes.
- Placed copies are not edited in place. A change goes through a Library and then an Update from Library preview.
