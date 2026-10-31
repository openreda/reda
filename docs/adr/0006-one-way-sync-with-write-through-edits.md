# Sync runs one way, from schematic to PCB; Component edits on the PCB write through to the schematic

The PCB Document only ever catches up with the Schematic Sheets: an explicit Sync applies a reviewed Change List from schematic to board, and there is no reverse Change List. Designator, value and footprint can still be edited from the PCB (including re-annotating by board position), but each such edit is a cross-Document command that writes the schematic Units and the Footprint instance together, recording an undo entry in each Document. We chose this so the schematic stays the single master assumed by ADR-0004: PCB copies can lag behind the schematic but never get ahead of it, so no difference ever needs a direction or a conflict resolution.

## Considered Options

- **Two-way back-annotation** (PCB copies may diverge; the difference list lets the user pick a direction per item, as commercial integrated suites do): rejected. It covers the same edits (Designator, value, footprint) but makes every item a potential two-way conflict and leaves two masters for Component data.
- **Live sync** (every schematic edit appears on the board at once): rejected. Half-finished schematic edits would churn a placed and routed board, and there would be no review step.

## Consequences

- The engine continuously computes the pending Change List so the UI can show that the PCB is out of sync; outputs warn but are not blocked.
- Sync never deletes copper: copper on a removed Net becomes unassigned, and Tracks of a removed Footprint stay, for DRC to report.
- Deleting a Schematic Sheet removes its Footprints only at the next Sync, as ordinary removals.
- Mechanical parts placed only on the PCB have no schematic master; they are edited on the PCB and Sync ignores them.
