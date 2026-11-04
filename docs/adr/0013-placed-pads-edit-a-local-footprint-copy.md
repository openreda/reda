# Placed Pads edit a local Footprint copy

REDA lets a user edit the Pads of a Footprint placed on the PCB, as the leading suite does, which partly reverses ADR-0008's rule that placed copies are not edited in place. The edit gives the instance its own entry in the PCB's part table, keyed by content hash and recording `edited_from {uuid, hash}`, so copper, DRC and outputs need no new code path because they already read a copy. Sync compares the `edited_from` hash, so a local edit is neither out of sync nor reverted.

## Considered Options

- **Keep Pads read-only on the board:** simplest for Update from Library, but a one-off change to one placed part would need a new Library element.
- **A sparse list of Pad overrides on each instance:** Sync would be slightly simpler, but every geometry path would have to merge the overrides, and a field left off the list could not be edited.

## Consequences

- A placed Pad's `number` can't change, and Pads can't be added or removed, because Nets come from the schematic (ADR-0004).
- A Footprint change or Update from Library removes local edits, and its preview lists what is lost.
