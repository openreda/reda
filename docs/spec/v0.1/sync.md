# v0.1 schematic → PCB Sync

Sources: [How do the schematic and the PCB stay in sync?](https://github.com/openreda/reda/issues/11), [ADR-0006](../../adr/0006-one-way-sync-with-write-through-edits.md), [ADR-0004](../../adr/0004-connectivity-derived-from-schematic-documents-own-their-data.md). Sync is cross-Document work, so it lives in `reda-engine`.

## Trigger and direction

- **Trigger:**
  - Changes reach the PCB only through an explicit **Update PCB from Schematic** (Sync). There is no live sync.
  - The engine keeps computing the pending Change List. A badge on the PCB tab and in the status bar shows "out of sync: N changes".
  - Running outputs while the board is out of sync warns, but is allowed.
- **One way only.** The schematic is the sole master, and there is no reverse Change List. PCB copies can lag the schematic, never lead it, so no change ever needs a direction.

## Identity keys

- Components are matched by Component UUID, and Pads are found through the Part's pin-pad map.
- **Hierarchy constraint:**
  - Sync keys and Component references must allow a Sheet Instance path prefix (*path + UUID*). In v0.1 the path is always empty.
  - This is so v0.5 hierarchy can reuse one sheet several times without changing the keys ([milestones](https://github.com/openreda/reda/issues/36)).

## Change List

- **Grouping:** changes are grouped by kind:
  - add or remove a Component
  - change a Footprint
  - change a Designator, value, fields, In BOM, In CPL or Not Fitted
  - add, rename or remove a Net
  - change a Pad's Net
- **Ticking:**
  - Every change is ticked by default.
  - Unticking a change also unticks the changes that depend on it. For example, unticking "add Net X" unticks "Pad → X".
- **Nothing is stored:**
  - Unapplied changes simply show up again in the next preview.
  - There is no "ignore forever" list.
- **Apply** is one transaction per Document.

## Net matching

- **Matching:**
  - Each derived schematic Net matches the PCB Net it shares the most Pads with, by Component UUID and the pin-pad map.
  - The name breaks ties.
- **Split:** the part with the largest overlap keeps the PCB Net's UUID. The rest become new Nets.
- **Merge:** the PCB Net with the largest overlap survives. The others are removed, and their copper is reassigned to it.
- **Rename:** a name difference is a rename, not a delete and an add. That includes auto-names that follow Designators.
- New Nets are in the Default Net Class only. Net Classes stay on the PCB side.

- **Board names:** a Net's board name is its winning name, with the [sheet suffix](schematic.md#connectivity) when Append Sheet Numbers to Local Nets is on.

## Duplicate Nets

Source: [What are the schematic editing contracts?](https://github.com/openreda/reda/issues/132).

- **Blocking:** `sync.apply` refuses while a `duplicate_nets` Violation stands, and the Change List says why, as in the leading suite. `duplicate_nets` compares board names, so the suffix option is one fix and renaming a Net is the other.
- **Turned off:** with the check's severity Off, or the Violation waived, same-named Nets merge into one board Net.

## What Sync does to the board

- **Sync never deletes copper:**
  - Copper on a removed Net with no merge target becomes unassigned (no Net), and DRC flags it.
  - Removing a Component deletes its Footprint, but leaves any attached copper in place.
- **Footprint change:**
  - The new Footprint is swapped in place.
  - It keeps the position, rotation, side, lock and Designator text placement.
  - Pad Nets are reassigned from the new pin-pad map.
  - Tracks are left as they are, and DRC reports the dangling copper.
  - Any [local Pad edits](pcb.md#placed-pad-edits) are removed, and the Change List row says so.
- **Locally edited Footprints:** local Pad edits are physical data that the schematic doesn't hold. Sync and the out-of-sync check compare the entry's `edited_from.hash`, so an edit is neither out of sync nor reverted.
- **Components with no schematic counterpart:**
  - Only Mechanical Parts can be placed directly on the PCB. They carry no Component link, and Sync ignores them.
  - A Mechanical Component on a sheet syncs only if it has a Footprint choice. Without one it stays on the schematic, raises no warning, and can still reach the [BOM](outputs.md#bom).
  - A Net Tie syncs exactly like a Standard Component ([checks](checks.md#net-ties)).
  - A Standard Footprint whose Component is gone is proposed for removal.
- **Deleting a Schematic Sheet:** its Footprints are removed at the next Sync, as ordinary removals.
- **Duplicate Designators:**
  - They don't block Sync, because Sync matches by UUID.
  - ERC reports them as an error, and Output Jobs refuse to run while they exist.

## Placement of new Footprints

Each Sync places its new Footprints in a fresh cluster:
- **Where:**
  - outside the Board Outline, beside earlier clusters
  - at the origin if there is no outline
- **Grouping:**
  - grouped by Schematic Sheet, in sheet order
  - a multi-unit Component goes with the sheet of its first Unit
- **Order:** sorted by Designator within each group.

## Write-through edits

- Designator, value, fields, In BOM, In CPL, Not Fitted and Footprint can be edited from the PCB Properties panel, as cross-Document **write-through** Commands. Each one updates every Unit and the Footprint instance together, with one undo entry in each Document.
- Re-annotate's "by board position" mode is built the same way.
- Mechanical parts that exist only on the PCB are edited there, fields, In BOM, In CPL and Not Fitted included.
- Pin and gate swaps flowing back from the PCB are deferred to v0.5.

## Cross-select and Net highlighting

- **Both are view state.** They are not undoable and never dirty a Document.
- **Selection** mirrors by Component UUID, between Units and the Footprint. Units on sheets that are not in view are highlighted without switching tabs.
- **Net highlighting** maps a schematic Net to a PCB Net through the same matching that Sync uses.
- Both are on by default, and can be switched off.
- Un-synced Components have nothing to select on the other side.

## Commands

- **`sync.preview`** (query):
  - It returns the Change List as structured data.
  - Each change has a stable id: a deterministic hash of its kind and targets.
  - The list as a whole has a hash.
- **`sync.apply`** (edit):
  - It takes an optional set of change ids to leave out, plus the preview's hash.
  - It fails with `conflict` if the Change List no longer matches the preview.
- The GUI's Change List dialog and MCP/CLI use the same pair of Commands.
