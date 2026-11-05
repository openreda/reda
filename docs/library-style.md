# Library style

The conventions of the Starter Library, checked by tests over it. They also guide user Libraries, and the Symbol Wizard and Footprint Generators follow them. Source: [How deep do the Library editors, generators and checks go?](https://github.com/openreda/reda/issues/135).

## Symbols

- Pins sit on the 50 mil grid. Neighbouring IC Pins are 100 mil apart.
- Pin length is 100 mil, or 200 mil on large ICs.
- Bodies sit on the 50 mil grid.
- A rectangular IC body has Pin 1 at the origin. Op-amps and discretes are centred on the body.
- The Designator goes above the body and the Value below it.

## Footprints

- The origin is at the body centroid.
- Every Footprint has a closed Courtyard, an Assembly outline, an Assembly Designator and a `height`.
