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

**Part**:
A Library definition that combines a Symbol, its alternate Footprints with how Pins map to Pads, and default fields; Components are placed from Parts.
_Avoid_: Device, Library part, Component (that is the placed instance)

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
A drawn line on a Schematic Sheet that joins Pins and other Wires into the same Net.
_Avoid_: Trace, track (those are copper on the board), connection

**Junction**:
The dot marking where three or more Wire ends or Pins meet in one Net; it follows from the drawing and is never placed by hand.
_Avoid_: Node, dot, connection point

**Net Label**:
A named marker attached to a Wire that joins it to every other Wire with the same name on the same Schematic Sheet.
_Avoid_: Tag, net name, label (alone), local label

**Port**:
A named, directional marker attached to a Wire that joins it to the Ports of the same name on other Schematic Sheets of the Project.
_Avoid_: Global label, off-sheet connector, sheet connector

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

**Track**:
A drawn copper segment on a copper Layer that joins Pads, Vias and other Tracks of the same Net.
_Avoid_: Trace, wire (wires are on Schematic Sheets), route

**Via**:
A plated hole that joins copper on different Layers within one Net.
_Avoid_: Through-hole (that describes parts), feedthrough

**Zone**:
A Net-assigned area on a copper Layer that is filled with copper around everything it must keep clear of.
_Avoid_: Pour, polygon, plane, fill

**Keepout**:
An area of the board in which chosen kinds of object, such as copper or Vias, must not be placed.
_Avoid_: Restrict area, rule area, exclusion

**Ratsnest**:
The lines showing connections a Net still needs on the board that copper does not yet make.
_Avoid_: Airwires, connection lines, unrouted nets

**Net Class**:
A named group of Nets that share board design rules, such as wider Tracks for power.
_Avoid_: Net group, rule set

**Design Rule**:
One limit the board must meet, such as minimum clearance, applying board-wide or within a scope such as a Net Class.
_Avoid_: Constraint

**Placement Origin**:
The point on a PCB Document from which Component positions are measured in manufacturing outputs.
_Avoid_: Aux origin, drill origin, datum, zero point

## Manufacturing

**Fab Preset**:
A named description of what one fabrication house expects, such as its minimum design rules, file conventions and BOM columns.
_Avoid_: Fab profile, manufacturer settings, CAM settings

**Output Job**:
A saved, named recipe in a Project that produces a chosen set of manufacturing files for one PCB Document according to one Fab Preset.
_Avoid_: OutJob, export settings, plot settings, fab output

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

## Sync

**Sync**:
Bringing the PCB Document in line with the Schematic Sheets by applying a reviewed Change List; the schematic is always the source, never the board.
_Avoid_: Update, forward/back annotation, import changes, ECO

**Change List**:
The set of individual differences between the Schematic Sheets and the PCB Document that a Sync would apply, each of which can be included or left out.
_Avoid_: ECO, diff, differences

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
The searchable list of every Command and Preset available in the current context, opened from the keyboard.
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

**Active Bar**:
The compact floating toolbar over a Document's canvas that offers the most-used Tools for that Document type.
_Avoid_: Quick bar, floating toolbar, palette
