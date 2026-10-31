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
One placed instance of a Library part in a Project, shown as a Symbol on a Schematic Sheet and as a Footprint on the PCB Document.
_Avoid_: Part (when an instance is meant), device

**Pin**:
A connection point of a Symbol, corresponding to one or more pads of its Footprint.
_Avoid_: Terminal, port

**Net**:
A set of Pins that are electrically connected.
_Avoid_: Signal, connection, wire (a wire is only one way of drawing part of a Net)
