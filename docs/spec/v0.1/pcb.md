# v0.1 PCB editor

Source: [Which features make up the v0.1 PCB editor?](https://github.com/openreda/reda/issues/8). Crate: `reda-pcb` (copper connectivity, Ratsnest, DRC, Zone fill, Commands). The editor and Tools are in `reda-ui`.

## Board

- One PCB Document (`.redapcb`) per Project.
- **Board Outline:**
  - It is an object of its own.
  - It is drawn as a polygon or rectangle, or created from selected lines and arcs.
- **Board Cutouts** are separate objects.
- **Creating the board:**
  - New Project creates the PCB Document, with its rules and DRC severities seeded from the chosen Fab Preset ([app](app.md#new-project)).
  - It may start with a rectangular Board Outline from the New Project dialog. Otherwise the canvas shows a hint: "No Board Outline: draw one, or **Create rectangle…**" (`pcb.board_outline.add_rect {width, height, corner_radius?}`).
  - The PCB Document can be deleted after confirmation. **Add PCB Document** (`document.add {kind: pcb}`) is available only while the Project has none. Output Jobs that name a missing PCB are flagged as broken until they are re-pointed. A Project opened without its PCB file shows an error in Messages and offers Add PCB Document.
- The board owns its [Stackup](#stackup), edited in **Board Setup**.
- `fab_notes` is optional free text stored in the PCB Document, edited on the Stackup page of Board Setup with `pcb.fab_notes.set`. It is copied into [Fab Notes](outputs.md#fab-notes).
- The board owns its design rules and DRC severities ([checks](checks.md#design-rules)). A Fab Preset seeds them when the board is created. They are edited in **Board Setup** ([app](app.md#settings-dialogs)).

## Stackup

Sources: [Which structural format changes does v0.1 make now?](https://github.com/openreda/reda/issues/124).

- `.redapcb` holds a `stackups` table with one entry in v0.1, seeded when the board is created from the [Fab Preset](outputs.md#stackup-seeds-and-suggestions), or else 1.6 mm with 1 oz copper. It is set with `pcb.stackup.set`.
- **Stored values:**
  - `thickness`: the finished thickness you order.
  - `entries`, ordered top to bottom:
    - copper: `{layer, thickness}`
    - dielectric: `{uuid, thickness?}`
  - `finish`, `mask_color`, `silk_color`: optional text with suggestions from the job's Fab Preset, written as typed in [Fab Notes](outputs.md#fab-notes) and left out when unset.
- A 2-layer board is copper, one dielectric with no thickness, copper. Solder mask, silkscreen and paste are not entries.
- **Copper units:** thickness is stored in nm and can be entered in µm, mm, mil or oz, where 1 oz is exactly 35 µm. Fab Notes show both for a standard weight, such as "1 oz (35 µm)", and µm only otherwise.
- **Totals:**
  - A dielectric left blank takes the remainder of `thickness`. A negative remainder is an error on the field.
  - If every dielectric is typed in, `thickness` stays as stored and any mismatch shows as a warning.
- The Stackup page in Board Setup is an editable table beside a drawn cross-section, with copper, dielectric and total thickness updating as you type.

## Layers

- A fixed set:
  - Top and Bottom Copper
  - Silkscreen, Solder Mask and Paste, top and bottom
  - Courtyard and Assembly, top and bottom
  - the Board Outline
  - Mechanical and Documentation layers
- Layers are stored as a generic ordered list, so v0.2 multilayer needs no format migration ([architecture](architecture.md#design-data-model)).
- Dielectrics are Stackup entries, not Layers.

## Placement

- **Footprints arrive through Sync only**, in clusters outside the board, grouped per Schematic Sheet ([sync](sync.md#placement-of-new-footprints)).
- The only exception is Mechanical Parts, which are placed directly from the Library panel.
- Footprints can be placed on either side.
- **Moving and turning:**
  - Drag to move. Space rotates.
  - L flips to the other side, mirroring Pads and silk. An SMD Pad's [`side`](library.md#pad-model) swaps with it.
  - Rotate and flip also work while placing.
- **Arranging:**
  - Align and distribute.
  - A per-Component lock.
  - An **Arrange within rectangle** Command.
- **Cross-select** with the schematic, with its order kept for placing parts in the order they were picked ([sync](sync.md#cross-select-and-net-highlighting)).
- **Properties panel**, laid out as in [Properties with a selection](editing.md#properties-with-a-selection):
  - Designator, value and Footprint are editable here as write-through Commands ([sync](sync.md#write-through-edits)).
  - Each Footprint has a movable, rotatable Designator text.
- Pads come from the Library part and can be [edited on the board](#placed-pad-edits).
- **Free Pads** are plated or non-plated Pads with no Footprint, optionally on a Net. They serve as mounting holes and fiducials, as Mechanical Parts can. They have the whole [Pad model](library.md#pad-model).
- **Output Origin:** an optional object that sets where coordinates and [the grid](editing.md#grid) run from in the editor, and where a `relative` Fab Preset measures Gerber, drill and CPL coordinates from ([outputs](outputs.md#output-origin)).

### Placed Pad edits

Sources: [How do mask, paste and tenting inherit, and what can a Pad express?](https://github.com/openreda/reda/issues/126), [ADR-0013](../../adr/0013-placed-pads-edit-a-local-footprint-copy.md).

- `pcb.pad.edit {instance, pad, ...}` changes one Pad of one placed Footprint, from the Properties panel. Every Pad property can be edited except `number`.
- Pads can't be added or removed on the board, and `number` stays fixed, because Nets come from the schematic.
- **A locally edited Footprint:**
  - The edit gives the instance its own entry in the PCB's part table, keyed by the new content hash.
  - The entry records `edited_from {uuid, hash}`, naming the copy it came from. Pads keep their UUIDs.
  - Instances with identical edits share one entry.
  - Copper, DRC and outputs read this entry as they read any other copy.
- Properties marks the instance as locally edited, and the DRC report lists these instances.
- Sync keeps local edits. A Footprint change removes them, including an [Update from Library](library.md#update-from-library) or Replace Part that changes that Footprint's hash ([sync](sync.md#what-sync-does-to-the-board)). An update that leaves that hash alone, such as Fields only, keeps them.
- **The compare**, shown in those previews and from Properties, as in the leading suite:
  - It lists Pad, property, and edited value against original, and draws the instance's Pads over the copy they came from.
  - A user includes or excludes a whole Component. There is no per-Pad merge.
- **Reverting:** `pcb.footprint.revert_local_edits {instances}`, also from Properties, returns an instance to the `edited_from` copy, not to the Library. The PCB's table keeps a copy while an entry's `edited_from` names it. The leading suite's revert goes to the library. REDA's goes to the copy, because one-way Sync means the board should match the schematic's copy, and the Library may have moved on.

### Update from Library on the board

Source: [What exactly do Update from Library and Replace Part change?](https://github.com/openreda/reda/issues/134).

A Mechanical Part placed directly on the PCB stores only a Footprint copy and its instance's fields, with no link to its Part and no Component.

- **PCB menu ▸ Update from Library** (`pcb.footprint.update_preview`, `pcb.footprint.update_from_library`) covers these instances only, by Footprint hash, with the same compare and a `preview_hash`, as in the leading suite's PCB compare.
- It keeps the position, rotation, side, lock and Designator text placement. Fields aren't touched.
- A Footprint no registered Library holds shows as Source Missing and can't be updated.
- There is no Replace Part here. `pcb.footprint.set`, or deleting and placing, does that job.
- Instances with a Component update through the [schematic](library.md#update-from-library) and Sync.

## Ratsnest

- A live minimum spanning tree per Net.
- It can be shown or hidden per Net and globally.
- A line stays 1 px wide at every zoom, so it can't be mistaken for a Track. It dims with Net highlighting, like the rest of the board.
- A connection drops out of the Ratsnest once copper makes it.
- [Net highlighting](editing.md#net-highlight) works across the schematic and the PCB.

## Routing

Source: [How should the Track Tool and Track dragging behave?](https://github.com/openreda/reda/issues/136), tried in a [throwaway prototype](https://github.com/openreda/reda/tree/prototype/track-tool). It follows the leading suite except where stated.

Manual single Tracks. Nothing shoves.

### Starting a route

- **On a Pad, Via or Track:** the route takes its Net and Layer. Copper on the active Layer wins, then copper on any other Layer, whose Layer becomes active.
- **On a Ratsnest line:** the route jumps to the line's nearer end, and to that end's Net and Layer. Copper under the cursor wins over a line.
- **In free space:** a no-Net route at the board-wide preferred width. DRC reports [Copper with no Net](checks.md#drc-checks).
- **Keys before the first segment is placed:**
  - `9` jumps to the other end of the connection.
  - `7` drops this connection and takes the next one leaving the same object.
  - `L` switches to the next copper Layer of a through-hole Pad or Via.

### Preview

| State | Drawn | Meaning |
|---|---|---|
| Placed | solid | Held by the Tool, and Backspace can remove it |
| Proposed | translucent | The next click places it |
| Look-ahead | hollow | The next click doesn't place it |
| Blocked | thin ghost to the cursor, with an X at its end | Can't be reached in `stop` mode |
| Removed by loop removal | red | Goes when the route ends |

- **Look-ahead (`1`, on by default):** a click places only the first of the two segments to the cursor. Off, it places both. A click that will end the route places both either way.
- **The red preview is REDA's addition**, so the consequence of a re-route shows before the click that makes it.
- **Proposed segments are translucent, not hatched,** which is what was tried and reads clearly over copper at any zoom.

### Corners

- `/` cycles 45°, 90° and Any Angle. `Space` flips which leg comes first. Any Angle has no second leg.
- The leading suite cycles with Shift+Space. REDA keeps `/`, which the Keymap already binds on every Document.
- Arc corners are v0.4. Their mode names `45_arc` and `90_arc` and the radius keys `,` and `.` are reserved.

### Placing and ending

- **Commit:** a click only adds to the Tool's list. When the route ends, the Tool runs one `batch`: loop-removal deletes, then `pcb.track.add` and `pcb.via.add`. It is one Journal entry and one undo step, and a single Track with nothing else runs as a plain `pcb.track.add`.
- **Backspace** removes the last placed vertex.
- **Ways a route ends:**
  - **Auto-terminate (`Shift+A`, on by default):** reaching a same-Net Pad, Via or Track ends it. Off, a re-route can run along an existing path.
  - **Enter or a double-click** ends it at the last point.
  - **Esc or right-click** ends it and keeps what is placed.
- **The Tool stays active after a route ends.** A second Esc or right-click returns to Select. With only the start point placed, Esc drops the route and stays in the Tool, so a wrong start costs one key.
- **Zones don't end a route.** In a [`zones_only`](#pour-over-same-net-copper) Zone of its Net, the Tool shows "not connected: end with a Via or Enter". A Track ending there isn't connected, and the Ratsnest stays.
- `Tab` opens the routing values in Properties: width, Via size and Layer.

### Obstacles

- **Modes (`Shift+R`, also while dragging):**
  - **`stop`** (default): the proposed path is clipped at the last clear point. A click places only up to there, and that point may be off grid, because pulling back to the grid could stop short of a gap the route fits through.
  - **`ignore`:** the route goes anywhere, drawn in the violation colour. It can be placed, and DRC reports it.
- The mode is remembered in `pcb.route.obstacle_mode`. `walkaround`, `push` and `hug_push` are reserved for v0.4.
- **What the Tool obeys:** clearance, `edge_clearance`, a Keepout's `restrict` set with normal clearance unless [`zero_keepout_clearance`](#keepouts) is on, and the [Net Tie rule](checks.md#net-ties), the same one DRC uses.
- **Clearance display (`C`):** other-Net copper grown by its clearance is shaded. `C` cycles `off`, `near` (within 5 mm of the cursor, the default) and `all`, kept in `pcb.route.clearance_display`. The leading suite uses `Mod`+W, which closes tabs in most apps.

### Width

- **Where it starts:**
  1. On a Track, the route takes that Track's width. `pcb.route.pickup_width` turns this off.
  2. Otherwise it takes the value of the current mode for the Net, normally [`preferred`](checks.md#which-value-applies). A route from a bare Pad never looks at the Net's other copper.
- **Modes (`3`):** `min`, `preferred`, `max` and `user`, the last chosen value. An absent limit or an empty `user` is skipped. The mode is kept in `pcb.route.width_mode`, and `preferred` is the default.
- **Chooser (`Shift+W`, or the width on the [Routing Bar](app.md#routing-bar)):** a list and a typed field. It lists `min`, `preferred`, `max`, the favourites in `pcb.route.favourite_widths` and the widths already on the board. Widths on the board are REDA's addition, so a hand-set width is easy to return to. The leading suite lists only favourites here.
- **Clipping:** a typed or chosen value is clipped to the Net's `min` and `max` with a status note, unless the limit is absent.
- A change applies from the last placed vertex on. Per-Net remembered widths and Via sizes are v0.4, reserved as `routing {width, via}` on the Net.

### Loop removal

When a route ends by joining two points already joined on its Net, the old path goes in the same `batch`. Re-routing is then just drawing the new path, which is what makes a router that doesn't shove pleasant.

- **The old path** is the shortest same-Net path between the route's two ends that passes through no Pad. A Track is split where needed, and what stays is re-added in the batch.
- **It stays, with a note, when:**
  - the new route is narrower than the old path's widest Track
  - a real branch joins it, including a Via touching a Zone. The note says to delete it by hand. Taking part of the path could remove copper nobody meant to touch. The survey doesn't record what the leading suite does here.
- **Antennas:** a dangling stub on the old path goes too, unless it ends in a Via. `pcb.route.remove_antennas` turns this off.
- **Vias:** a path through Vias goes with its Vias. `pcb.route.remove_loops_through_vias` turns this off.
- **Off switches:**
  - `Shift+L` toggles it in the Tool, and `pcb.route.loop_removal` holds the default. A loop made while it is off stays when it comes back on. The leading suite uses Shift+D, which is the [HUD](editing.md#hud)'s delta line here.
  - Per Net, `loop_removal` on the Net (default on, left out of the file when on), set with `pcb.net.set_routing {nets, loop_removal}` and shown in Properties for selected copper. It is for power and ground Nets that need loops.

### Dragging Tracks

- **Slide:** a plain drag on a segment moves it along its normal. A lone neighbour keeps its line and angle, and shortens or lengthens.
- **Past a neighbour's end:** the neighbour shrinks to nothing and goes, a collinear segment beyond it merges, and the drag stops there. It doesn't build a new corner, which keeps accidental kinks out.
- **Fixed ends:** where a neighbour can't follow (a Pad, a branch, or a parallel neighbour), a connector is added: 45° with **Include Miters** (`C` while dragging, on), else square.
- **Merge Collinear (`R` while dragging, on):** a segment that lines up with a neighbour merges into it.
- **Obstacles:** the same mode and key as routing. In `stop`, the slide stops at the last clear position.
- **Corner:** dragging a vertex uses `Space` to pick Deform (the default, where the vertex follows the cursor) or Scale (the corner keeps its shape).
- **Break:** drag a selected segment's midpoint handle, or `Mod`+Shift+press at a point, to add a vertex there.
- **Track end:**
  - A free end moves alone.
  - A vertex shared by two segments moves both.
  - An end on a Footprint Pad detaches, and the Ratsnest shows the gap.
  - An end on a Free Pad or Via moves that object with all its Tracks.
- **Via and Free Pad:** a plain drag keeps every Track attached.
- `Mod`+drag detaches, and `pcb.drag.preserve_angle` (on) turns angle keeping off.
- T-junction drags and multi-Track end moves are v0.4.

### Selecting and clearing copper

- **`Tab` with copper selected and no Tool in flight** widens the selection a step at a time: contiguous Track on this Layer, connected copper, the whole Net, then back to the start. A double-click stays Properties, as for [every object](editing.md#stacked-objects).
- **`U`** selects connected copper directly.
- **Commands:**
  - `pcb.selection.connected {seeds, scope: layer|copper|net}`
  - `pcb.selection.physical_connection {seed, single_layer?}`: Track and Vias up to the next Pad
  - `pcb.selection.footprint_connections {footprints}` and `pcb.selection.footprint_nets {footprints}`
- **Unwind:** Backspace with one selected end object deletes it and selects its neighbour, passing through Vias and stopping at a Pad. If it touches several objects, it is deleted with nothing selected. `Mod`+Del unwinds both ways.
- **`pcb.route.unroute {all?, nets?, connection?, footprints?, include_locked?}`:** locked copper stays unless `include_locked`, which the GUI asks about.
- **`pcb.track.cleanup {nets?}`** removes stacked duplicates and merges collinear segments.
- **Selection rectangles have no PCB-only gestures.** The [Selection Filter](editing.md#selection-filter) picks Tracks under a Footprint. Ratsnest lines aren't selected: they hide per Net.

### The proposal function

The Tool's preview comes from one pure function in `reda-pcb`, so v0.4 modes plug in without a new Tool:

```text
propose(copper, {start, cursor, mode, corner, posture, lookahead, width, layer, net})
  -> {place, look_ahead, blocked, status: clear|clipped|violating, ends_route, removes, note}
```

- `mode` is the open obstacle enum. Each v0.4 mode is one more arm.
- Placed segments stay in the Tool until the route ends, so v0.4 glossing can rework them.
- It reads the Copper index and changes nothing, so it can be tested with no window.

### Room for later

Push-and-shove, differential pairs, length tuning and curved Tracks: Tracks are single segments with a Line|Arc geometry, and v0.1 creates only Lines.

## Vias

- Through Vias only. The model stores a layer span on every Via, so blind, buried and micro Vias can come later.
- **In a route:** each places the Via at the cursor, drawn before the click.
  - `V`, or `+`, `-` and `*`: place a Via and switch Layer.
  - `2`: place a Via and stay on this Layer.
  - `E`: place a Via and end the route, for dropping into a pour. The leading suite uses numpad `/`, which many laptops don't have.
- **Sizes:** they start at the Net's [preferred `via_size`](checks.md#which-value-applies). `4` cycles `min`, `preferred`, `max` and `user`, and `Shift+V` opens a chooser with the sizes already on the board. A typed size is clipped to `min` and `max`.
- **On its own, for stitching:** a Via takes the Net of the copper under its centre on the active Layer, else the Net set in the Tool's Properties, else none.
- A Via has the same `mask {top?, bottom?, from_hole_edge?}` as a [Pad](library.md#pad-model), with `top` and `bottom` as board sides. An untented side opens at the Via's diameter plus its expansion.

## Mask and paste

Source: [How do mask, paste and tenting inherit, and what can a Pad express?](https://github.com/openreda/reda/issues/126).

- **Board values** are two [rule records](checks.md#design-rules), edited on the Mask & Paste page of Board Setup:
  - `mask_expansion`: `{top, bottom, from_hole_edge, tent_vias_top, tent_vias_bottom}`
  - `paste_expansion`: `{expansion}` or `{percent}`
- **Inheritance:**
  - A Pad's or Via's own value wins, side by side.
  - Otherwise the rule applies, [resolved through the Net's classes](checks.md#which-value-applies). A Pad on no Net uses the board-wide record.
  - Tenting from the rule applies to Vias only. A Pad is tented by its own `mask`.
  - `from_hole_edge` from the rule applies only to Pads and Vias with holes.
- There is no Footprint-level default. A Footprint sets values on its Pads, and `footprint` is a reserved scope filter.
- **Resolved openings:** one function in `reda-pcb` works out each opening. Gerbers, silkscreen clearing and DRC all use it.
  - **Mask:** one opening per side for each plated or non-plated Pad and untented Via, and one on its own side for an SMD Pad. A non-plated Pad's opening comes from its shape. A tented side has none.
  - **Paste:** SMD Pads only, on the Pad's side, from the expansion, percent or windows. `none` has no opening.
  - An opening with nothing left after a negative value is not plotted, and DRC reports it.

## Zones

Source: [How deep do Zones, Keepouts and the outline editor go?](https://github.com/openreda/reda/issues/137). REDA follows the leading suite except where a difference is stated with its reason.

- A solid fill on a Net, on one copper Layer.
- **A Zone holds:**
  - `name`: optional. Left out, a name is [derived](#zone-names).
  - `layer` and `net`. A Zone may have no Net.
  - `vertices`, or `follows_outline: true` with no vertices ([from the Board Outline](#creating-zones)).
  - `clearance`: optional. Left out, the clearance rule applies. If it is set, the larger of it and the rule applies. The leading suite uses a scoped rule instead. v0.1 scopes accept only the board or a Net Class, so this field stands in until a `zone` scope arrives.
  - `min_width`: optional. Left out, it is the Net's `track_width` min. Thinner necks are removed, and outer fill corners are rounded at half this width, which follows from the algorithm and has no control of its own.
  - `pour_over`: how the fill treats same-Net copper.
  - `remove_unconnected` and `min_island_area`: [islands](#islands).
  - `priority`. Where Zones overlap on a Layer, the higher priority fills first and the others keep clear. A tie goes to the lower UUID.
  - `shelved`, left out when false.
- `fill_mode` is reserved. v0.1 has only solid fills.

### Pour over same-Net copper

| `pour_over` | Same-Net Zones | Same-Net Tracks | Pads and Vias |
|---|---|---|---|
| `zones_only` (default) | merge | keep clear by the clearance | by [Zone connection](#zone-connections) |
| `all` | merge | merge | by Zone connection |
| `none` | keep clear by the clearance | keep clear by the clearance | by Zone connection |

- With `zones_only`, a Track leaving a relieved Pad can't bypass the relief. A Track that ends inside the Zone isn't connected to it, and the Ratsnest stays until the Track reaches a Pad or Via.
- Connectivity follows the copper actually made.

### Zone connections

How a fill joins a Pad or Via of its own Net comes from the [`zone_connection` rule](checks.md#rule-records). A Zone holds no connection values, as in the leading suite. A power Net that wants solid connections gets them through its Net Class.

- **Values:** `through_hole`, `smd` and `via`, each with:
  - `style`: `relief`, `solid` or `none`
  - `spokes`: 2 or 4
  - `spoke_width`
  - `gap`: the clear ring around a relieved or `none` Pad or Via
  - `angle`: 0, 45 or 90 degrees in the object's own frame. Angle 0 puts spokes along its axes. 90 only differs from 0 with 2 spokes.
- **Seeds:** `through_hole` and `smd` relief, 4 spokes, 0.25 mm width, 0.25 mm gap, angle 0. `via` solid, with the same spoke values.
  - The survey proposed 45°. A 45° spoke leaves a rectangular Pad at its corner, so 0 is the default. It behaves like the leading suite's Auto on rectangular Pads.
- **Which value applies, value by value:** the [Pad's `zone_connection`](library.md#pad-model), then the highest-ranked Net Class record that sets it, then the board-wide record. A Free Pad behaves like a Pad. Vias have no per-object override in v0.1.
- **`none`:** the fill keeps `gap` clear of the object and doesn't connect to it.
- A spoke that can't reach the fill is dropped. A relieved object with no spoke left isn't connected, so the Ratsnest shows it.

### Islands

- `remove_unconnected` (default on) removes pieces of fill that touch no copper of the Net. It has no effect on a Zone with no Net.
- `min_island_area` (default 0) removes any piece smaller than the area, even a connected one.
- They are independent, as in the leading suite.
- DRC warns about what they leave: [`dead_copper`](checks.md#drc-checks) for unconnected pieces that stay, and `empty_zone` for a Zone with nothing left to fill.

### Zone names

- A Zone with no `name` shows a derived one, which is never stored. It follows the Net, the Layer order and the Zone's order, so it updates when they change. Clearing a typed name restores the derived one.
- The board's `zone_name_scheme`, on [Board Setup ▸ Zone Connections](app.md#settings-dialogs), is one of `NET_LXX_PXXX` (default), `LXX_NET_PXXX`, `NET_LAYERNAME_PXXX` and `LAYERNAME_NET_PXXX`.
- `P` numbers count the Zones sharing that Net and Layer, in priority order. `NONET` stands in for a missing Net.
- Names are unique on the board. A typed name that matches another Zone's name is rejected.
- Properties shows the outline area and the filled copper area, read-only.

### Creating Zones

- **Drawing:** the Zone Tool's options appear in Properties. It pre-fills them with the last-used values, kept in a Preference, as in the leading suite. The general object defaults page is v0.2.
- **`pcb.zone.add`:** a parameter left out takes a fixed value (`zones_only`, `remove_unconnected` on, `min_island_area` 0, `clearance` and `min_width` left out), never the Preference. So the same Command gives the same result on any machine, and every value is stored on the Zone, so a Preference can't change an output.
- **From the Board Outline:** `pcb.zone.add_from_board_outline {net, layers}` makes one Zone per Layer with `follows_outline`.
  - The fill follows later outline edits and keeps `edge_clearance`, so there is no inset value.
  - The leading suite's equivalent makes a one-time copy, which goes wrong when the board is resized.
  - **Detach** (`pcb.zone.detach_outline`) turns it into ordinary vertices. Until then it has no vertex handles.
- **From a selection:** see [closed loops from graphics](#closed-loops-from-graphics). A Zone lands on the active copper Layer.

### Filling

- Fills go in the `.reda/` cache, never in the Document ([architecture](architecture.md#file-format)). How they are drawn is in [app](app.md#canvas-rendering).
- **`pcb.zone.fill {zones?}`:** the named Zones and those that depend on them, or every stale Zone if left out.
- **Automatic refill:** the Preference `pcb.zone.auto_refill` is `after_edit` (default) or `off`.
  - After a committed edit, the affected Zones and the lower-priority Zones overlapping them refill in the background. A fill never runs during a drag.
  - Each Zone has its own input hash, so staleness is per Zone.
- Outputs and DRC always refill stale Zones first, so the Preference can't change them.
- **Arcs** are flattened to chords at a fixed 0.001 mm.
  - It is a constant, not the setting the survey proposed. It only trades file size against fidelity, far below what a fab can resolve.
  - True-arc fills are [later](../modules.md#later-fog). Fills are only in the cache, so they need no format change.

### Pour order

- The **[Zones panel](app.md#window-shell)** lists Zones per Layer, in priority order, with name, Net, area, stale marker and a shelve checkbox.
- Dragging a row or using **Auto order** changes the order, and each change is one undo step. Reordering lives in a Panel, not the leading suite's modal manager, because you reorder while watching the canvas.
- **Commands:** `pcb.zone.reorder {layer, order}`, `pcb.zone.bring_to_front`, `pcb.zone.send_to_back` and `pcb.zone.auto_order {layer?}`, which puts the smallest Zones first on each Layer.

### Shelving

- `pcb.zone.shelve {zones?}` and `pcb.zone.restore {zones?}`, with all Zones if left out.
- A shelved Zone stays in the file but is hidden from the canvas and skipped by fill, DRC and connectivity. It is restored from the Zones panel, or with Restore All, and then refills.
- Outputs leave it out. DRC's `shelved_zone` Warning flags each one, so a forgotten Zone can't drop copper from Gerbers silently.

### Zone Cutouts

- A **Zone Cutout** is its own object, `{uuid, layer, vertices}`, as in the leading suite. It isn't owned by one Zone.
- It removes the fill of every Zone on its Layer inside its shape, exactly to its edge.
- It is never plotted.
- **Commands:** `pcb.zone_cutout.add`, `set`, `delete` and `add_from_graphics`.
- A Keepout restricting `zones` differs: it applies clearance and can cover all copper Layers.

## Keepouts

- **A Keepout holds:**
  - `layer`: one copper Layer, or left out for all copper Layers, so "all" still means all when v0.2 adds Layers. This replaces today's list.
  - `shape`: `polygon {vertices}` or `line {vertices, width}`. Rectangle and circle are Tool modes that store polygons.
  - `restrict`: any of `tracks`, `vias`, `zones`, `smd_pads`, `tht_pads` and `footprints`. A new Keepout has all but `footprints`, and at least one is required. This replaces `copper` and `vias`.
- `footprints` is REDA's own addition, for mounting-hole and connector areas. It checks the Courtyard, or the [fallback box](checks.md#courtyards), of a Footprint on the Keepout's Layer side, with no added clearance. A Keepout on all Layers applies to both sides.
- **Clearance:** copper keeps the normal clearance rule from a Keepout, as in the leading suite. The `clearance` rule's board-wide `zero_keepout_clearance` (default off) makes every Keepout edge-exact.
- Keepouts have no Net and are never plotted.
- **Commands:** `pcb.keepout.add`, `set`, `delete` and `add_from_graphics`.
- Keepouts inside Footprints are [reserved](architecture.md#reserved-names).

## Polygon editor

One editor serves the Board Outline, Board Cutouts, Zones, Zone Cutouts and polygon Keepouts, as in the leading suite.

- **Corner handles:** move vertex, chamfer (a bevel whose midpoint follows the cursor) and fillet (the same as an arc). A key cycles them while dragging.
- **Edge-midpoint handles:** move edge, keeping the adjacent angles; split, adding a vertex; and bow, making the edge an arc.
- Dragging an edge away from a handle slides it.
- `Mod`+click on an edge inserts a vertex. Del while holding a vertex removes it.
- Each drag is one undo step.
- **Alignment guides** to the adjacent edges and other vertices use the alignment level of the [shared snapping](editing.md#cursor-snapping).
- **Commands**, taking any of these objects:
  - `pcb.polygon.set_vertices {object, vertices}`
  - `pcb.polygon.fillet {object, vertices?, radius}`, on all corners if left out
  - `pcb.polygon.chamfer {object, vertices?, distance}`, on all corners if left out
  - Fillet and chamfer are REDA's own additions. They also round the corners of a finished rectangle.
- **Self-intersection:** every Command rejects a self-intersecting shape and reports the crossing point. In the editor the shape shows as invalid during the drag and returns to the last valid shape on release. This differs from the leading suite's Proceed or Revert prompt: a kept self-intersecting shape breaks fills and Gerbers, and no workflow needs one.

### Closed loops from graphics

- `add_from_graphics` works the same for `pcb.board_outline`, `pcb.board_cutout`, `pcb.zone`, `pcb.zone_cutout` and `pcb.keepout`.
- Line and arc ends must meet within 0.001 mm.
- `mode` is `centerline` (default) or `outer_edge`. On a gap, the Command is rejected with the gap's location and size, and the GUI offers `outer_edge` as a retry.
- The source graphics are removed unless `keep` is set, which differs from the leading suite's always keeping them, so that all five Commands behave the same.

## Moving the board

- **`pcb.board.move {dx, dy}`** moves the Board Outline, its Board Cutouts, the Output Origin and every object with any part inside the outline, as in the leading suite. A Track crossing the edge moves whole. Objects wholly outside stay, such as Footprints in unplaced Sync clusters.
- **Move Board Shape** is a menu item that starts an ordinary move of the outline alone, with the [rotate and mirror keys](editing.md#rotation). Board Cutouts stay unless they are selected too.

## Text and graphics

- Free text and lines, arcs and rectangles on the silk, copper and mechanical Layers.
- Text on bottom Layers can optionally be mirrored.
- **Fonts:**
  - The default is the vendored Hershey Roman Simplex stroke font, drawn as capsules.
  - TTF/OTF text uses bundled Noto Sans (OFL) or user-installed fonts.
  - Characters Hershey lacks fall back, glyph by glyph, to Noto Sans outlines.
  - A text object names its font family. If that font is missing on this machine, the text draws as Noto Sans, Messages warns, and Output Jobs that plot it warn.
  - Sources: [rendering](https://github.com/openreda/reda/issues/26), [ADR-0002](../../adr/0002-permissive-licensing-and-third-party-data.md).

## View

- **A dockable Layers panel:**
  - show/hide and colour per Layer
  - an active-Layer selector
  - the Inactive-Layer mode: Normal, Dimmed or Hidden
  - a view-from-bottom flip
  - visibility per object type
- Colour schemes are fixed per app theme. Colour overrides are Preferences ([app](app.md#preferences)).
- **Units:** mm by default, with a one-key mm/mil toggle.
- **Grid and snapping** follow the [grid](editing.md#grid) and [cursor snapping](editing.md#cursor-snapping) rules.
- **Origin:** the [Output Origin](outputs.md#output-origin) is the user-set origin. Coordinates, Go To and the grid run from it.
- A measure tool.

## Deferred past v0.1

Push-and-shove, differential pairs, length tuning and curved Tracks (v0.4); 4+ Layers, the full Stackup editor tab, blind/buried/micro Vias and teardrops (v0.2); 3D view and DXF (v0.3); multi-board Projects (v0.5); autorouter; hatched and true-arc Zones, Rooms, and scoped or query rules; image logos; arrays and pattern placement; printing to paper.
