# v0.1 schematic → PCB Sync

Sources: [How do the schematic and the PCB stay in sync?](https://github.com/openreda/reda/issues/11), [ADR-0006](../../adr/0006-one-way-sync-with-write-through-edits.md), [ADR-0004](../../adr/0004-connectivity-derived-from-schematic-documents-own-their-data.md). Sync is cross-Document work, so it lives in `reda-engine`.

## Trigger and direction

- **Trigger:**
  - Changes reach the PCB only through an explicit **Update PCB from Schematic** (Sync). There is no live sync.
  - The engine keeps computing the pending Change List. A badge on the PCB tab and in the status bar shows "out of sync: N changes", leaving out [Problem rows](#row-status).
  - Running outputs while the board is out of sync warns, but is allowed.
- **One way only.** The schematic is the sole master, and there is no reverse Change List. PCB copies can lag the schematic, never lead it, so no change ever needs a direction.

## Identity keys

- Components are matched by Component UUID, and Pads are found through the Part's pin-pad map.
- **Hierarchy constraint:**
  - Sync keys and Component references must allow a Sheet Instance path prefix (*path + UUID*). In v0.1 the path is always empty.
  - This is so v0.5 hierarchy can reuse one sheet several times without changing the keys ([milestones](https://github.com/openreda/reda/issues/36)).

## Change List

Source for this section's depth: [How deep do the Change List and Sync dialogs go?](https://github.com/openreda/reda/issues/131).

- **Grouping:** one group per kind, as in the leading suite, so a whole kind can be included or left out at once. In order: Add Components, Remove Components, Change Footprints, Designators, Fields (value included), In BOM, In CPL, Not Fitted, Add Nets, Rename Nets, Remove Nets and Pad Nets. Kind ids are stable. Per-kind "include by default" Project Settings are reserved for v0.5.
- **Ticking:**
  - Every change is ticked by default, except Problem rows, which can't be ticked.
  - Unticking a change also unticks the changes that depend on it. For example, unticking "add Net X" unticks "Pad → X".
- **Nothing is stored:**
  - Unapplied changes simply show up again in the next preview.
  - There is no "ignore forever" list.
- **Apply** is one transaction per Document.
- **No configurable matching:** Net matching has no thresholds or manual match dialog, unlike the leading suite. The fixed overlap-then-name rule makes a wrong match rare, and a setting would make the same Project sync differently per configuration.

### Row status

`sync.preview` gives every row one status, with a message unless it is OK. The leading suite has only pass or fail, and needs a separate Validate step. REDA's preview already validates, so there is none.

- **OK:** applies, with nothing to say.
- **Note:** applies, with a consequence worth reading:
  - a removed Net or Component leaves copper in place, with a count of what becomes unassigned or stays attached
  - a Footprint change removes [local Pad edits](pcb.md#placed-pad-edits) or leaves dangling copper
  - a [likely relink](#relink): a remove and an add have the same Designator and Footprint
- **Problem:** can't apply, so its box is disabled and `sync.apply` skips it. Its dependants are left out too. There are two cases:
  - a fitted Standard or Net Tie Component with no Footprint choice
  - a Pin on a Net with no matching Pad through the pin-pad map

Problem rows don't count towards "out of sync: N", so the count can reach zero when a Part is deliberately built that way. The leading suite counts failed rows as changes. REDA differs because a permanent count would also trigger the out-of-sync warning on every Output Job. [ERC](checks.md) and [Output Jobs](outputs.md) report the missing Footprint on their own.

The [`duplicate_nets` block](#duplicate-nets) is not a row status. It is a banner over the whole list, with Apply disabled.

### Change List dialog

A modal tree grid, opened from Update PCB from Schematic, the PCB tab badge and the status bar badge.

- **Columns:** tick, Kind, Target, From → To, Status.
- **Group headers:** a count and a tick-all box.
- **Toolbar:** text filter, **Only Problems** (Problem and Note rows), Expand All, Collapse All, Include All and Exclude All.
- **Rows:** multi-select, and Space toggles the tick.
- **Context menu:** Include Selected, Exclude Selected, Include All of This Kind, Exclude All of This Kind, Invert and Show.
- **Footer:** Export, Apply and Close.
- **Show** reuses [Show on board](app.md#settings-dialogs): the dialog hides and keeps its ticks, the row's objects are highlighted, the canvas is view-only, and a banner offers **Back to Change List**.
  - The banner also switches between the board and the sheet.
  - A row with nothing on one side, such as Add Component on the board, opens the side that has it.
  - The leading suite keeps its modal dialog on screen while cross-probing. REDA hides it, because the dialog would otherwise cover what it shows. A non-modal surface is the live pending-changes Panel reserved for v0.2.
- **Export** writes the Change List through [`checks.report`](checks.md#report-files), in the same formats as an Output Job's `reports/`.
- **After Apply:**
  - The dialog stays open. Applied rows stay visible, greyed, with ✓ Applied until Close, as in the leading suite.
  - The other rows come from a fresh plan and keep their ticks, and Apply can run again.
  - There is no per-row Failed state, because Apply is one transaction. A stale list shows one `conflict` banner and refreshes. One Sync also never needs a second run to finish, unlike in the leading suite.

### Relink

Every [paste](schematic.md#cut-copy-and-paste) gives new UUIDs, so cut and paste shows up as a removal and an addition, losing placement and routing.

- **v0.1:** when a Remove Component and an Add Component row have the same Designator and Footprint, both get a Note saying that placement and routing will be lost, and that undoing and using [Move to Sheet](schematic.md#move-to-sheet) keeps them.
- **Reserved for v0.2:** the change kind `relink_component`. It replaces that pair of rows with its own row, which moves the Component link and keeps the Footprint, its placement and its copper. Unticking it falls back to the removal and the addition.

[Move to Sheet](schematic.md#move-to-sheet) needs nothing from Sync. UUIDs stay, so it makes no Component rows, only any Net renames that follow from the move.

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

Each Sync places its new Footprints in a fresh cluster. The leading suite makes a stored placement area per sheet instead. REDA has no such object in v0.1, and the same workflow needs only these rules and a Command. The width and gap below are REDA's own, since no rule for them could be found to follow.

- **Where:**
  - to the right of the Board Outline and of everything already placed
  - at the origin if there is neither
- **Blocks:**
  - one block per Schematic Sheet, stacked top to bottom in sheet order
  - a multi-unit Component goes with the sheet of its first Unit
- **Inside a block:**
  - rows in natural Designator order (R2 before R10)
  - on the top side, at rotation 0
  - 1 mm between Courtyards, using the box around Pads and graphics when a Footprint has no Courtyard
  - a row's target width is √(sum of the boxes' areas) × 1.5
- **Cluster label:** a sheet-name label above each sheet's Footprints that lie entirely outside the Board Outline.
  - It is view-only, derived live and never saved, so renaming a sheet updates it and it disappears as parts move onto the board.
  - With no outline, every Footprint counts as outside.
  - Two Syncs of one sheet share one label, at the group's top-left.
- **Selecting a sheet's parts:** a sheet tab's context menu has **Select Footprints of This Sheet**.

## Write-through edits

- Designator, value, fields, In BOM, In CPL, Not Fitted and Footprint can be edited from the PCB Properties panel, as cross-Document **write-through** Commands. Each one updates every Unit and the Footprint instance together, with one undo entry in each Document.
- Re-annotate's "by board position" mode is built the same way.
- Mechanical parts that exist only on the PCB are edited there, fields, In BOM, In CPL and Not Fitted included.
- Pin and gate swaps flowing back from the PCB are deferred to v0.5.

## Cross-select and Net highlighting

- **Both are view state.** They are not undoable and never dirty a Document.
- **Selection** mirrors by Component UUID, between Units and the Footprint. Units on sheets that are not in view are selected and highlighted without switching tabs.
  - The mirrored objects are really selected, so board Commands can act on what was picked on the schematic.
  - Edit Commands act only on the active Document's objects, so Delete or Move on a mirrored selection never changes the other Document.
- **Order:** mirrored objects follow pick order, with one Footprint per Component, at its first picked Unit. Inside a single box-select, where no pick order exists, they follow natural Designator order. The [place queue](pcb.md#placement) reads this order, as in the leading suite.
- **Net highlighting** maps a schematic Net to a PCB Net through the same matching that Sync uses.
- Both are on by default, and can be switched off.
- Un-synced Components have nothing to select on the other side.
- **Cross-select [Preferences](app.md#preferences),** with the leading suite's meanings:
  - **Dim** others on the target side (on).
  - **Zoom** to the target (off).
  - **Focus** the other Document if it is visible (on).
  - **Reposition** (off): picking a Unit attaches its Footprint to the PCB cursor when the PCB canvas is visible. It skips locked Footprints.
- **Cross-probe** is an explicit Command in the context menu. It always zooms and dims the target, and its jump option makes the other tab active. As in the leading suite, cross-select doesn't zoom and cross-probe does.

## Commands

- **`sync.preview`** (query):
  - It returns the Change List as structured data.
  - Each change has a stable id: a deterministic hash of its kind and targets.
  - Each change has a [status](#row-status) and, unless it is OK, a message.
  - The list as a whole has a hash, a count that leaves out Problem rows, and any `duplicate_nets` block.
- **`sync.apply`** (edit):
  - It takes an optional set of change ids to leave out, plus the preview's hash.
  - It skips Problem rows and reports the ids it applied and left out.
  - It fails with `conflict` if the Change List no longer matches the preview.
- The GUI's Change List dialog and MCP/CLI use the same pair of Commands.
