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
A collection of reusable part definitions from which Components are placed.
_Avoid_: Part database, catalog

**Symbol**:
The schematic representation of a part, with its Pins.
_Avoid_: Schematic part, glyph

**Footprint**:
The physical land pattern of a part on the board.
_Avoid_: Land pattern, package, decal

**Component**:
One placed instance of a Library part in a Project, shown as one or more Units on Schematic Sheets and as a Footprint on the PCB Document.
_Avoid_: Part (when an instance is meant), device

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
A set of Pins that are electrically connected.
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
