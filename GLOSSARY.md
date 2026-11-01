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

**Component**:
One placed instance of a Part in a Project, shown as one or more Units on Schematic Sheets and as a Footprint on the PCB Document.
_Avoid_: Part (when an instance is meant), device

**Component Type**:
The kind of a part that decides whether its Components go into the BOM and whether they may exist on the board without a schematic counterpart, such as Standard or Mechanical.
_Avoid_: Part class, category, BOM flag

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
The state of a Component that stays in the design and on the board but is left out of assembly.
_Avoid_: DNP, DNF, do not populate, excluded

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
