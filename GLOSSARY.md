# REDA

An electronic design automation (EDA) tool: one application in which a user captures a circuit, lays out the board that realises it, and produces the files to manufacture it.

## Design structure

**Project**:
The unit of work: one electronic design and everything that belongs to it.
_Avoid_: Workspace, design, solution

**Document**:
A single editable part of a Project, opened as a tab in the application, such as a Schematic Sheet or a PCB Document.
_Avoid_: File, page, view

**Schematic Sheet**:
A Document that expresses a circuit's logical connectivity using Symbols and wires.
_Avoid_: Schematic page, diagram

**PCB Document**:
A Document that expresses the physical board: its outline, layers, placed Footprints and copper.
_Avoid_: Board file, layout file

**Panel Document**:
A Document that arranges copies of the Project's own boards, with rails and break-offs, into one manufacturable panel.
_Avoid_: Panel file, array, step-and-repeat

**Gerber Document**:
A read-only Document that shows a set of Gerber and drill files, whether REDA wrote them or not.
_Avoid_: CAM file, Gerber view

**Waveform Document**:
A Document holding the results of a circuit simulation as plotted signals.
_Avoid_: Simulation results, plot, scope

**Sheet Instance**:
One use of a Schematic Sheet within a hierarchy; a sheet used several times has one Sheet Instance per use, each with its own Designators and Net names.
_Avoid_: Channel, sheet copy, repeat

## Parts

**Library**:
A collection of reusable Symbols, Footprints and Parts from which Components are placed.
_Avoid_: Part database, catalog

**Symbol**:
The schematic representation of a part, with its Pins.
_Avoid_: Schematic part, glyph

**Footprint**:
The physical land pattern of a part on the board.
_Avoid_: Land pattern, package, decal

**Courtyard**:
The closed area a Footprint keeps clear around its part, covering the body and Pads plus a margin; two parts are too close when their Courtyards overlap.
_Avoid_: Keepout (that is a board object), placement outline

**Footprint Generator**:
A Command that calculates a Footprint for one package family from datasheet dimensions.
_Avoid_: Footprint wizard, IPC wizard

**Symbol Wizard**:
The dialog and Command that lay out a Symbol's Pins and body from a list of Pins.
_Avoid_: Symbol generator

**Pin Decoration**:
A glyph drawn on a Pin at one of four positions, such as a clock or an inversion dot.
_Avoid_: Pin symbol, pin shape

**Active Library**:
A registered Library that is scanned and searched; one switched off stays registered, and Components placed from it read Source Missing.
_Avoid_: Enabled library, loaded library

**Part**:
A Library definition that combines a Symbol, its alternate Footprints with how Pins map to Pads, and default fields; Components are placed from Parts.
_Avoid_: Device, Library part, Component (that is the placed instance)

**Library Status**:
How a placed Component's copy compares with its Part in the Libraries: Up to Date, Library Differs or Source Missing.
_Avoid_: Library newer, out of date, not found

**Full Replace**:
An Update from Library that takes the Symbol, the Fields and the Footprints together.
_Avoid_: Full update, overwrite

**Locked Field**:
A field on a Component that Update from Library leaves alone; it can still be edited by hand.
_Avoid_: Pinned field, read-only field

**Provenance**:
Where an imported Library element came from: its source service, the service's terms URL and any copyright fields, recorded on the element.
_Avoid_: Source, origin, attribution

**Component**:
One placed instance of a Part in a Project, shown as one or more Units on Schematic Sheets and as a Footprint on the PCB Document.
_Avoid_: Part (when an instance is meant), device

**Component Type**:
The kind of a part that decides whether its Components may exist on the board without a schematic counterpart and how checks treat their copper, such as Standard, Mechanical or Net Tie. It also sets the defaults for In BOM and In CPL.
_Avoid_: Part class, category

**In BOM**:
Whether a Component is a line to buy on every build, and so is listed in the BOM. A Part sets the default and each Component can change it.
_Avoid_: Exclude from BOM, No BOM

**In CPL**:
Whether a Component is placed by the assembler's machine, and so is listed in the CPL. A Part sets the default and each Component can change it.
_Avoid_: Exclude from position file, pick-and-place flag

**Net Tie**:
A Component Type whose Footprint deliberately joins two or more Nets through its own copper, while the Nets stay separate in the design.
_Avoid_: Net bridge, star point, jumper (a fitted link is a different thing)

**Unit**:
One independently placeable section of a Symbol, such as one amplifier of a dual op-amp or its power section; a single-unit Symbol has exactly one.
_Avoid_: Gate, sub-part, section

**Unit Suffix**:
The letter or number drawn after a Designator to show which Unit a symbol is, such as the A in U1A; it is not part of the Designator.
_Avoid_: Part letter, sub-part ID

**Autoposition**:
The state of a Component's field or Footprint text whose position REDA works out by itself, which lasts until the text is moved by hand.
_Avoid_: Auto-place

**Designator**:
The human-readable reference that names a Component in a Project, such as R1 or U3; it can change without changing which Component it is.
_Avoid_: Reference, refdes, RefDes, ID

**Pin**:
A connection point of a Symbol, corresponding to one or more pads of its Footprint.
_Avoid_: Terminal, port

**Net**:
A set of Pins and Pads that are electrically connected; on Schematic Sheets it follows from the drawing, while the PCB Document keeps its own Nets in step with the schematic through sync.
_Avoid_: Signal, connection, wire (a wire is only one way of drawing part of a Net)

## Schematic drawing

**Wire**:
A drawn line of one or more segments on a Schematic Sheet that joins Pins and other Wires into the same Net.
_Avoid_: Trace, track (those are copper on the board), connection

**Junction**:
The dot marking where Wires meet in a T or a 4-way, or where a Wire passes over a Pin, Port or Power Port; it follows from the drawing and is never placed by hand.
_Avoid_: Node, dot, connection point

**Normalisation**:
The tidying an edit Command does to the Wires it touches, in the same undo step: merging, cutting at a dropped Component, splitting at Junctions and offsetting 4-ways, as the wiring Preferences (or the Command's `wiring` parameter) say.
_Avoid_: Cleanup, optimisation (Optimize Wires is one of its steps)

**Net Label**:
A named marker attached to a Wire or Pin that joins it to every other Wire with the same name on the same Schematic Sheet.
_Avoid_: Tag, net name, label (alone), local label

**Local Net**:
A Net that stays on one Schematic Sheet, with no Port or Power Port.
_Avoid_: Sheet net, private net

**Port**:
A named, directional marker attached to a Wire that joins it to the Ports of the same name on other Schematic Sheets of the Project.
_Avoid_: Global label, off-sheet connector, sheet connector

**Port Cross-Reference**:
The note drawn beside a Port that says which sheets and locations hold the other Ports of the same name; it follows from the design and is never stored.
_Avoid_: Sheet reference, off-page reference

**Border Zone**:
A square of a Schematic Sheet's border grid, named by its row and column such as B3, that gives a location on the sheet; it has nothing to do with a copper Zone.
_Avoid_: Zone, grid reference

**Power Port**:
A named symbol, such as GND or 3V3, that joins its Wire to the Project-wide Net of that name on every Schematic Sheet.
_Avoid_: Power symbol, power flag, rail symbol

**No-Connect Marker**:
A mark on a Pin stating that it is intentionally left unconnected.
_Avoid_: No-ERC, NC flag, X

## Board

**Board Outline**:
The closed shape that defines the physical edge of the board in a PCB Document.
_Avoid_: Board shape, edge cuts, outline layer

**Board Cutout**:
A closed shape inside the Board Outline where the board material is removed.
_Avoid_: Slot (a slot is a hole), hole, void

**Layer**:
One plane of the PCB Document on which objects sit, such as Top Copper, Bottom Silkscreen or Board Outline.
_Avoid_: Plane (alone), sheet, level

**Stackup**:
The ordered physical build of a PCB Document: its copper and dielectric entries with their materials and thicknesses.
_Avoid_: Layer stack, build-up, layer setup

**Dielectric**:
An insulating entry in the Stackup between copper. It holds no objects, so it is not a Layer.
_Avoid_: Dielectric layer, substrate layer

**Pad**:
A copper area, with or without a hole, through which a Footprint (or a free-standing hole) connects to the board; each Pad belongs to at most one Net.
_Avoid_: Land, pin (pins are on Symbols)

**Free Pad**:
A Pad placed directly on the board without belonging to any Footprint, such as a mounting hole.
_Avoid_: Standalone pad, hole object

**Locally Edited Footprint**:
A placed Footprint whose Pads or graphics were changed on the PCB, so that it differs from the copy it came from.
_Avoid_: Footprint override, modified footprint

**Mask Expansion**:
How far a solder-mask opening extends past the edge of a Pad, Via or hole; a negative value makes the opening smaller.
_Avoid_: Mask margin, mask clearance, mask swell

**Tented**:
Covered by solder mask with no opening, said of one side of a Pad or Via.
_Avoid_: Masked, covered, capped (capping is a different via protection)

**Paste Expansion**:
How much a solder-paste opening differs from a Pad's copper, as a length or a percentage of area; a negative value makes the opening smaller.
_Avoid_: Paste margin, paste clearance, paste reduction

**Paste Window**:
One of the openings that together replace a Pad's solder-paste opening.
_Avoid_: Paste aperture, stencil opening, paste pad

**Track**:
A drawn copper segment on a copper Layer that joins Pads, Vias and other Tracks of the same Net.
_Avoid_: Trace, wire (wires are on Schematic Sheets), route

**Via**:
A plated hole that joins copper on different Layers within one Net.
_Avoid_: Through-hole (that describes parts), feedthrough

**Zone**:
A Net-assigned area on a copper Layer that is filled with copper around everything it must keep clear of.
_Avoid_: Pour, polygon, plane, fill

**Zone Connection**:
How a Zone's fill joins a Pad or Via of its own Net: by relief spokes, solidly, or not at all.
_Avoid_: Thermal, connect style

**Zone Cutout**:
A shape on a copper Layer inside which every Zone on that Layer leaves out its fill.
_Avoid_: Zone hole, Board Cutout (that removes board material)

**Shelved**:
Said of a Zone kept in the file but hidden and left out of fill, DRC, connectivity and outputs.
_Avoid_: Disabled

**Keepout**:
An area of the board in which chosen kinds of object, such as copper or Vias, must not be placed.
_Avoid_: Restrict area, rule area, exclusion

**Look-ahead**:
The last proposed segment of a route or Wire, which shows where the next one goes and isn't placed by the next click.
_Avoid_: Preview tail

**Loop removal**:
Deleting the old path when a finished route joins two points already joined on its Net.
_Avoid_: Auto-cleanup, re-route

**Ratsnest**:
The lines showing connections a Net still needs on the board that copper does not yet make.
_Avoid_: Airwires, connection lines, unrouted nets

**Net Class**:
A named, ranked group of Nets that share board design rules, such as wider Tracks for power. A Net can be in several, and the higher rank wins where they disagree.
_Avoid_: Net group, rule set

**Default Net Class**:
The Net Class every Net is in, whose values are the board-wide design rules. It ranks below every other Net Class.
_Avoid_: All Nets, base class

**Preferred Value**:
The size a Tool starts with for a Track or Via, within the limits its design rule sets.
_Avoid_: Default width, routing width

**Rules File**:
A file holding a board's design rules, DRC severities and Net Class names, for use on another board.
_Avoid_: Rule set, constraint file

**Design Rule**:
One limit the board must meet, such as minimum clearance, applying board-wide or within a scope such as a Net Class.
_Avoid_: Constraint

**Dimension**:
A drawn measurement on the board between two points or objects, whose value follows them.
_Avoid_: Ruler, measurement

**Knockout Text**:
Text shown as gaps in a filled box, rather than as ink.
_Avoid_: Inverted text, negative text

**Text Frame**:
A box that free text wraps inside.
_Avoid_: Text box

**Lock**:
A mark on a placed object that protects its geometry from being changed by accident.
_Avoid_: Freeze, pin

**Layer Set**:
A named choice of which Layers are shown.
_Avoid_: Layer preset, layer group

**Pinned Layer**:
A Layer that stays shown in single-Layer mode.
_Avoid_: Sticky layer

**Output Origin**:
A point the user can place on a PCB Document, from which the editor measures coordinates and the grid, and from which manufacturing files measure theirs when the Fab Preset asks for a relative origin.
_Avoid_: Placement Origin, user origin, current origin, aux origin, drill origin, datum, zero point

## Manufacturing

**Fab Notes**:
A plain-text file in an Output Job's zip telling a person at the fab what the board is, such as its thickness, finish, minimum features and deliberate shorts.
_Avoid_: README, fab drawing, order notes

**Output Manifest**:
The file beside an Output Job's zip that records what the run wrote, with a hash of each file and of the zip, so what was sent can be proved later.
_Avoid_: Release manifest, file list

**Fab Preset**:
A named description of what one fabrication house expects, such as its minimum design rules, file conventions, BOM and CPL form and packaging. It says nothing about any one build, which is the Output Job's part.
_Avoid_: Fab profile, manufacturer settings, CAM settings

**Output Job**:
A saved, named recipe in a Project that produces a chosen set of manufacturing files for one PCB Document according to one Fab Preset.
_Avoid_: OutJob, export settings, plot settings, fab output

**Name Template**:
The text with tokens that names an Output Job's zip and its BOM and CPL files.
_Avoid_: Name expression, file pattern

**Not Fitted**:
The state of a Component that stays in the design and on the board but is left out of assembly on this build. In BOM says whether it is ever a line to buy; Not Fitted says this build leaves it off.
_Avoid_: DNP, DNF, do not populate, excluded

**Variant**:
A named, Project-level set of per-Component overrides (Not Fitted, value, Part substitution) that yields one buildable version of the design.
_Avoid_: Assembly variant, configuration, build option

## Checking

**ERC**:
The Electrical Rule Check: a check of the Schematic Sheets for connections that are electrically suspect, such as two Outputs driving one Net.
_Avoid_: Validation, verify, electrical check

**DRC**:
The Design Rule Check: a check of the PCB Document against its design rules, such as clearance and Track width.
_Avoid_: Validation, verify, board check

**Violation**:
One breach of an electrical or design rule reported by a check, naming the objects involved.
_Avoid_: Error (that is one severity), marker, issue, DRC error

**Waiver**:
A recorded acceptance that a particular Violation is intentional, so checks stop reporting it.
_Avoid_: Exclusion, ignore, suppression

**Stale Waiver**:
A Waiver that no longer matches any Violation, because its objects no longer break its check.
_Avoid_: Orphan waiver, expired waiver

**Pin Matrix**:
The Project's table of how strongly ERC reports each pairing of Pin types and Port directions on a Net, and each Pin type left unconnected.
_Avoid_: Connection matrix, pin-type grid

**Health Check**:
A DRC check for data that breaks no design rule but harms outputs, such as a zero-length Track.
_Avoid_: Monitor, cleanup check

## Sync

**Sync**:
Bringing the PCB Document in line with the Schematic Sheets by applying a reviewed Change List; the schematic is always the source, never the board.
_Avoid_: Update, forward/back annotation, import changes, ECO

**Change List**:
The set of individual differences between the Schematic Sheets and the PCB Document that a Sync would apply, each of which can be included or left out.
_Avoid_: ECO, diff, differences

**Cluster**:
The group of new Footprints one Sync places outside the board, arranged per Schematic Sheet, for the user to place from.
_Avoid_: Room, placement area

**Cross-select**:
Selecting a Component on the schematic or the board also selects it on the other side, in the order picked.
_Avoid_: Linked selection

**Cross-probe**:
An explicit request to show an object's counterpart on the other side, zoomed to and with everything else dimmed.
_Avoid_: Jump, locate

## Working with the design

**Command**:
A named user-visible action, such as adding a Wire or running a Sync, that every way of driving REDA (menus, shortcuts, scripts, the command line, AI agents) invokes in the same way.
_Avoid_: Action, operation, function

**Tool**:
An interactive mode, such as Wire drawing or routing, that follows the pointer with a live preview and finishes by carrying out a Command.
_Avoid_: Mode (alone), editor, interactive command

**Preset**:
A named Command with some parameters fixed, such as the Wire Tool's activation, that has its own title and icon and can be placed in menus, toolbars and shortcuts like a Command.
_Avoid_: Variant, macro, action

**Keymap**:
The set of shortcuts that map key chords to Commands and Presets, in contexts from the active Tool through the Document type to the whole application; users override it on top of the shipped defaults.
_Avoid_: Hotkeys, key bindings (as a whole), accelerators

**Command Palette**:
The searchable list opened from the keyboard or the command field in the top row, which finds Commands and Presets, Components, Nets and Documents by prefix.
_Avoid_: Command bar, quick open, launcher

**Journal**:
The record of every Command carried out in the current session, with its parameters, where it came from and its result, which can be exported and run again to repeat the session.
_Avoid_: History, log, macro, undo stack

**Recovery Snapshot**:
A copy of a Document's unsaved changes that REDA keeps so they can be restored after REDA closes unexpectedly; it never replaces the Document's own file unless the user restores and saves it.
_Avoid_: Autosave, backup

**Crash Report**:
A local record of an unexpected REDA failure that the user may choose to share.
_Avoid_: Bug report, dump, telemetry

## Application window

**Panel**:
A shared, dockable view of the Project or the selection, such as Projects, Properties or Messages, that sits beside the Documents in the one window and serves whichever Document is active.
_Avoid_: Pane, sidebar, view (alone), tool window

**Dock**:
A strip of columns along the left, right or bottom edge of a window that holds tabs, usually Panels; a column can collapse to an icon strip whose icons open its tabs as flyouts.
_Avoid_: Sidebar, drawer, edge panel

**Layout**:
The arrangement of tabs, splits, Docks and windows in which a user works on a Project; a user can save a Layout under a name and apply it to any Project.
_Avoid_: Workspace, perspective, arrangement

**Preferences**:
A user's own choices about how REDA looks and behaves, such as the theme or repeat placement, which apply to every Project they open and never change a design.
_Avoid_: Settings, options, config

**Project Settings**:
A Project's own settings, such as its title-block properties, ERC severities and Fab Preset copies, which travel with the Project and apply to everyone who opens it.
_Avoid_: Project options, Project preferences

**Board Setup**:
A PCB Document's Stackup, design rules, Net Classes and DRC severities, edited together.
_Avoid_: Rules editor, board settings, constraints

**Scene**:
A Document drawn as a flat list of primitives in draw order, built from the design data and handed to the canvas, image export or PDF; it holds no design data of its own.
_Avoid_: Render tree, display list, view model

**Board navigator**:
The Panel that lists a board's Nets, Footprints or Zones, one mode at a time; its Zones mode is the Zones panel.
_Avoid_: PCB panel, Nets panel, object browser

**Layer control**:
The always-visible way to switch the active Layer over a board's canvas, either a swatch dropdown or a strip along the bottom, as a Preference chooses; it does not replace the Layers panel.
_Avoid_: Layer tabs, Layer bar

**Settings window**:
A non-modal window holding Preferences, Project Settings, Board Setup, an Output Job or the Change List, which stays open while the canvas is in use.
_Avoid_: Settings dialog, modal, options dialog

**Active Bar**:
The compact floating toolbar over a Document's canvas that offers the most-used Tools for that Document type.
_Avoid_: Quick bar, floating toolbar, palette

**Routing Bar**:
The bar at the bottom of the canvas while the Track Tool is active, showing and switching its modes.
_Avoid_: Track toolbar, mode bar

**Placement Bar**:
The bar at the bottom of the canvas while Footprints are selected or moving, showing and switching how they move.
_Avoid_: Move toolbar, placement toolbar

**HUD**:
A small readout by the cursor on a canvas, showing where the cursor is and how far it has moved since the last click.
_Avoid_: Overlay, cursor tooltip

**Selection Filter**:
The per-kind switches that decide which kinds of object a click or rectangle can pick; filtered kinds stay visible.
_Avoid_: Pick mask, selection mask

**Hotspot**:
A point on an object that the cursor snaps to, such as a Pad centre or a Wire end.
_Avoid_: Snap point, anchor

**Snap Distance**:
How close the cursor must come to a Hotspot on a board or Footprint before it snaps there, as a length.
_Avoid_: Snap radius, capture range

**Electrical Range**:
How close the cursor must come to a Hotspot on a sheet or Symbol before it snaps there, as a length set per grid preset.
_Avoid_: Electrical grid, snap radius

**Dimmed**:
Shaded to push it back visually, while it can still be picked and edited.
_Avoid_: Faded, greyed

**Masked**:
Greyed so that it can't be picked or edited until the mask is cleared.
_Avoid_: Locked, disabled, filtered
