# v0.2.5 built-in catalogue

Source: [Which form factors ship in the v0.2.5 built-in catalogue?](https://github.com/openreda/reda/issues/397). The numbers come from the research: [official mechanical specs](https://github.com/openreda/reda/issues/394) ([findings](https://github.com/openreda/reda/blob/research/form-factor-specs/docs/research/form-factor-specs/README.md)) and [card edges from open hardware](https://github.com/openreda/reda/blob/research/card-edge-open-hardware/docs/research/card-edge-open-hardware.md). Amended by [card-edge fingers](https://github.com/openreda/reda/issues/399) (the M.2 half-moon), [drawings](https://github.com/openreda/reda/issues/404) (drawing groups) and the [decisions taken while writing this spec](scope.md#decisions-taken-while-writing-this-spec).

This file lists every built-in template in the Starter Library, with its parameters, geometry, Parts, Keepouts and drawings, and the new starter Parts they need. Every number that reaches fab also goes in the [dimensions table](testing.md#spec-numbers) with its sources.

## Rules for every built-in

- **Format.** Each is a `.redaboard` in the Starter Library's `board-templates/`, written in the [same format](template-format.md) users write.
- **Layers.** `suggested_layers` is 2 for every built-in.
- **Coordinates.** All values are mm. The origin is the lower-left corner of the outline's bounding box, seen from the top (component side), with +X right and +Y up.
- **Numbers only.** Ship our own numbers, never vendor files, figures or text. Every number that reaches fab has two independent sources or one primary spec, or is a single-source value a ticket accepted knowingly ([dimensions table](testing.md#spec-numbers)). The only exception is the approximate host drawings on the Documentation Layer ([drawings](drawings.md#built-in-drawings)).
- **Connectors and holes.**
  - Every connector and hole starts Locked ([creation](creation.md#locked-state)).
  - "Pad 1 at (x, y) → pad n at (x′, y′)" fixes the connector's position, rotation and side. The implementer works out `at` and `rotation` from the Footprint's own pad layout.
  - Header and socket rows are through-hole, so `side` decides only which face the body is on.
- **Board finish.** No built-in sets `hard_gold`. The card-edge descriptions recommend it for cards that are inserted often ([edge connectors](edge-connectors.md#the-edge_connector-board-property)).

### Naming

- A name refers to the host only, and puts the trademark last ("Shield for Arduino Uno").
- A name never claims "HAT" or "HAT+". HAT+ has no fixed outline, and "Mini HAT+" doesn't exist.
- Compatibility wording and trademark acknowledgements go in `description`, for example "Raspberry Pi is a trademark of Raspberry Pi Ltd".

### Picker order

The picker groups templates by `category`. Generic comes first and the rest are alphabetical. User templates with no category go under "Other".

| Category | Templates |
|---|---|
| Generic | Custom rectangle, Eurocard |
| Adafruit Feather | Board in Adafruit Feather format |
| Arduino | Shield for Arduino Uno, Shield for Arduino Mega |
| PC expansion | M.2 card, PCI Express add-in card |
| Raspberry Pi | Add-on board for Raspberry Pi, Add-on board for Raspberry Pi Pico |

"No outline (draw it later)" is the picker's default entry under Generic. It is not a template: picking it sends no `board_template` ([dialog](creation-dialog.md#default)).

## Generic

### Custom rectangle

This replaces `project.new {board}` and the old dialog's *Rectangle* ([commands](commands.md#what-is-removed)).

| Parameter | Kind | Default | Range |
|---|---|---|---|
| `width` | length | 100 | 1 to 1000 |
| `height` | length | 80 | 1 to 1000 |
| `corner_radius` | length | 0 | 0 to 500 |

- **Outline:** a rectangle from (0,0) to (`width`, `height`) with `corner_radius` on all four corners. A radius of 0 drops the arcs. A radius larger than half a side fails [validation](template-format.md#after-evaluation).
- It has no holes, connectors, Keepouts or drawings.
- The defaults keep the old dialog's 100 × 80.

### Eurocard

| Parameter | Kind | Options | Default |
|---|---|---|---|
| `height` | choice | 3U (100), 6U (233.35) | 3U |
| `depth` | choice | 100, 160, 220, 280, 340, 400 | 160 |

- **Outline:** a rectangle `depth` wide (X) and `height` tall (Y), with square corners.
- **Outline only.** There are no holes, connectors or Keepouts. The DIN 41612 position, the rail strips and the board thickness sit behind the paywalled IEC text.
- The description says "outline only".
- **Key dimensions:** width and height.

## Adafruit Feather

### Board in Adafruit Feather format

There is one template with no `format` choice. It covers Feather mainboards and FeatherWings, which share the outline ([decisions taken while writing this spec](scope.md#decisions-taken-while-writing-this-spec)). The description says so.

- **Outline:** 50.8 × 22.86 with R2.54 on all four corners. The USB end is at X = 0.
- **Holes:** 4× `MountingHole_M2.5` at (2.54, 2.54), (2.54, 20.32), (48.26, 2.54) and (48.26, 20.32). Adafruit plates the two USB-end holes. The description says the user can swap in plated holes.
- **Connectors**, male, on the bottom:
  - `Header_1x16`: pad 1 at (44.45, 1.27) → pad 16 at (6.35, 1.27).
  - `Header_1x12`: pad 1 at (16.51, 21.59) → pad 12 at (44.45, 21.59).
- **Keepouts:** none.
- **Key dimensions:** width, height, the hole pitch (45.72 × 17.78) and the header row spacing (20.32).

## Arduino

Both templates use the exact outline from Arduino's CAD files: the 45° chamfered right-hand profile, with R1 on three corners. ICSP is omitted. Holes are `MountingHole_M3`, with no Keepouts, because Arduino publishes none.

### Shield for Arduino Uno

- **Outline**, counter-clockwise from the origin:
  - along the bottom to (66.04, 0), with R1 at (0,0) and at (66.04, 0);
  - up to (66.04, 2.54), then a 45° chamfer out to (68.58, 5.08);
  - up to (68.58, 37.846), then a chamfer in to (66.04, 40.386);
  - up to (66.04, 51.816), then a chamfer to (64.516, 53.34);
  - along the top to (0, 53.34), with R1 at that corner.
- **Holes:** (13.97, 2.54), (15.24, 50.8), (66.04, 7.62) and (66.04, 35.56).
- **Connectors**, male, on the bottom:

  | Part | Pad 1 | Last pad |
  |---|---|---|
  | `Header_1x10` (D8 to SCL) | (41.656, 50.8) | pad 10 at (18.796, 50.8) |
  | `Header_1x08` (D0 to D7) | (63.5, 50.8) | pad 8 at (45.72, 50.8) |
  | `Header_1x08` (power) | (27.94, 2.54) | pad 8 at (45.72, 2.54) |
  | `Header_1x06` (A0 to A5) | (50.8, 2.54) | pad 6 at (63.5, 2.54) |

  The D7 to D8 gap is 4.064 (160 mil). The D8 header is 1.016 off the 0.1 in grid that the other headers use.
- **Key dimensions:** width (68.58), height (53.34) and the header row spacing (48.26).

### Shield for Arduino Mega

- **Outline:** 101.6 × 53.34. The right-hand profile is the Uno's, shifted +33.02 in X.
  - The Mega CAD file puts its outline 0.0256 / 0.0128 off the grid that its holes and headers use. The template keeps the holes and headers on that grid and puts the outline at (0,0), so the Uno and Mega shields share header coordinates. The 26 µm difference is inside the dimensions table's tolerance.
- **Holes:** 6×, at (13.97, 2.54), (15.24, 50.8), (66.04, 7.62), (66.04, 35.56), (90.17, 50.8) and (96.52, 2.54).
- **Connectors**, male, on the bottom:
  - **The Uno's D8, D0 and power headers**, at the Uno positions.
  - **A0 to A7 in place of the Uno's A0 to A5.** On the Mega, the analog row starting at A0 is eight pins, and a 1×6 would share its holes. So the decided "Uno set plus 3× `Header_1x08`" is the Uno set with its `Header_1x06` replaced, plus two more 1×8 headers:

  | Part | Pad 1 | Last pad |
  |---|---|---|
  | `Header_1x08` (A0 to A7) | (50.8, 2.54) | pad 8 at (68.58, 2.54) |
  | `Header_1x08` (A8 to A15) | (73.66, 2.54) | pad 8 at (91.44, 2.54) |
  | `Header_1x08` (D21 to D14) | (86.36, 50.8) | pad 8 at (68.58, 50.8) |
  | `Header_2x18` (XIO) | (93.98, 50.8), pad 2 at (96.52, 50.8) | pads 35/36 at y = 7.62 |

- **Key dimensions:** width (101.6), height (53.34) and the header row spacing (48.26).

## PC expansion

### M.2 card

| Parameter | Kind | Options | Default |
|---|---|---|---|
| `length` | choice | 2230, 2242, 2260, 2280, 22110 (binding `card_length` = 30, 42, 60, 80, 110) | 2280 |
| `key` | choice | A, B, E, M, A+E, B+M | M |

- **Orientation:** the finger edge is at Y = 0. Pin 1 is toward +X, on the top (component) side.
- **Outline:**
  - The body is 22 wide, from Y = 4 to Y = `card_length`, with square far-end corners.
  - The finger tab is 19.85 wide, centred, from X = 1.075 to 20.925 and Y = 0 to 4. It has R0.5 shoulders where it meets the body.
- **Key notches:**
  - Each notch is 1.20 wide with a full-radius end, cut from Y = 0 to its tip at **Y = 3.50**. Do not use KiCad's 4.10.
  - Notch centres are at X = 11 + offset: A +6.625 (17.625), B +5.625 (16.625), E +2.625 (13.625) and M −6.125 (4.875).
  - Each notch is a vertex run of the outline, gated by `when` on `key`. A+E and B+M cut both of their notches.
- **Connector:** one card-edge Part per key, gated by `when` on `key`, at (11, 0) on the top side: `CardEdge_M.2_Key_A`, `_B`, `_E`, `_M`, `_AE` and `_BM` ([new Parts](#new-starter-parts)).
- **Half-moon:** `MountingHole_M2_Castellated`, a Mechanical Part at (11, `card_length`) and centred on the far edge ([new Parts](#new-starter-parts)).
  - The outline stays straight there. The fab's drill-then-rout makes the half-moon.
  - It is its own Includes tick. Its Pad has no Net until the user sets one, usually GND.
  - It is turned 180°, so its Designator lands on the card rather than past the far edge.
- **Keepout:** the component-free band on the bottom, `bottom_copper`, `restrict: [footprints]`, from Y = 0 to 5.20 across the full width. The card-edge Footprints draw their Courtyard on the top side only, so the band doesn't flag the card's own fingers.
- **The far-end 3.80 zone is a drawing, not a Keepout.** It is a labelled region from Y = `card_length` − 3.80 to `card_length` in the Key dimensions group, reading "RF receptacles or components". The source allows components there ([decisions taken while writing this spec](scope.md#decisions-taken-while-writing-this-spec)).
- **Board properties:** no `edge_connector` bevel, because no source gives one.
- **Description:** says the card is 0.8 mm thick, so the Fab Preset must match, and that the template doesn't set the thickness. It says no bevel figure is published, and recommends hard gold for cards that are inserted often.
- **Key dimensions:** width (22), length (`card_length`) and the notch centre from the centreline for the chosen key.

### PCI Express add-in card

| Parameter | Kind | Options or range | Default |
|---|---|---|---|
| `lanes` | choice | x1, x4, x8, x16 (binding `finger_long` = 8.15, 22.15, 39.15, 72.15) | x1 |
| `bracket` | choice | full height (binding `card_height` = 111.15), low profile (68.90) | full height |
| `card_length` | length | 57.15 + `finger_long` (the tab end: 65.30, 79.30, 96.30, 129.30) to 167.65 | 167.65 (half length) |

- **Orientation:** seen from the component side with the fingers down, the bracket is on the left. The B-side fingers are on top copper and the A-side on bottom copper.
  - The research's `u` axis runs from the key centreline toward the bracket, so X = 57.15 − `u`.
  - The bracket-side card edge is at X = 0, and the key centreline at X = 57.15. That is the 57.15 key-to-bracket-edge datum.
- **Outline**, from the bracket edge along the bottom:

  | Feature | X | Y |
  |---|---|---|
  | Bracket-end extension | 0 to 15.00 | bottom at 4.50 |
  | Cutout | 15.00 to 33.35 | up to 12.75 |
  | Tab | 33.35 to 41.35 | bottom at 4.50 |
  | Retention notch, full-radius R1.825 end | 41.35 to 45.00 | up to 12.75 |
  | Finger tab, 0.5 × 45° chamfers at both bottom corners | 45.00 to 57.15 + `finger_long` | bottom at 0 |
  | Body bottom edge | 57.15 + `finger_long` to `card_length` | 12.75 |
  | Far end | `card_length` | 12.75 to `card_height` |
  | Top edge | `card_length` to 0 | `card_height` |

  - Corners are square apart from the chamfers and the notch radius.
  - `card_length` starts at the chosen lane width's tab end, so a shorter length is refused as out of range ([#416 decision](https://github.com/openreda/reda/issues/416#issuecomment-7072198747)). A shorter card would still be a simple outline, with the tab sticking out past the body.
- **Key notch:** 1.90 wide at X = 57.15, cut from Y = 0 to its tip at Y = 8.40, with a full-radius R0.95 end.
- **Hole:** the lower bracket hole, `MountingHole_M3` (Ø3.2) at (7.50, 9.35).
  - The low-profile upper hole (7.50, 63.25) is verified but was not decided, so it is not included.
  - The full-height upper hole is out of scope.
- **Connector:** one card-edge Part per lane width, gated by `when` on `lanes`, with its origin on the key centreline at (57.15, 0), on the top side: `CardEdge_PCIe_x1`, `_x4`, `_x8` and `_x16`.
- **Board properties:** `edge_connector {bevel: {angle: 20°, depth: 1.30}}`, from TE drawing 2364427, the only source.
- **Description:** names the bevel source, recommends hard gold, and acknowledges "PCI Express and PCIe are trademarks of PCI-SIG", used descriptively.
- **Key dimensions:** width (`card_length`), height (`card_height`), the 57.15 key-to-bracket datum and the hole position.

## Raspberry Pi

### Add-on board for Raspberry Pi

This is built from the legacy HAT and uHAT drawings, which are BSD-3. It never claims "HAT" or "HAT+".

| Parameter | Kind | Options (bindings) | Default |
|---|---|---|---|
| `size` | choice | full (`base_height` 56, `top_hole_y` 52.5), micro (30, 26.5) | full |
| `gpio` | choice | through-hole (`smt_extra` 0), SMT (0.5) | SMT |
| `cutouts` | choice, shown only when `size` is full | display + camera, none | display + camera |

- **Outline:** 65 × (`base_height` + `smt_extra`), so 56.5 or 56 for full size and 30.5 or 30 for micro. It has R3 corners.
- **Display notch** (full size with cutouts): a notch in the left edge, X = 0 to 5 and Y = 19.5 to 36.5, with R1 inner corners.
  - It is part of the outline, so the template has three outline rows, because a gate can't say "micro, or full without cutouts":
    - the notched outline, `when: {size: [full], cutouts: [display_camera]}`;
    - the plain outline, `when: {size: [full], cutouts: [none]}`;
    - the plain outline, `when: {size: [micro]}`.
  - Exactly one survives each combination.
- **Camera slot** (full size with cutouts): a Board Cutout, an obround 2 × 17 overall, centred at (45, 11.5), so it spans Y = 3.0 to 20.0.
- **Holes:** 4× `MountingHole_M2.5` (2.7 drill, fitting the spec's 2.75 ± 0.05) at (3.5, 3.5), (61.5, 3.5), (3.5, `top_hole_y`) and (61.5, `top_hole_y`). Micro size also gets all 4.
- **Keepouts:** one per hole, a Ø6.2 circle on `all_copper` with `restrict: [tracks, vias, zones, smd_pads]`, `protects` the hole. `tht_pads` is left out so the Keepout doesn't flag the hole it protects.
- **Connector:** `Socket_2x20` on the bottom, centred at (32.5, `top_hole_y`), on the top-hole row.
  - Pad 1 is the left end of the row away from the board edge, at (8.37, `top_hole_y` − 1.27). Pad 2 is at (8.37, `top_hole_y` + 1.27).
- **Left out:** the PoE-header keepout, because it isn't dimensioned.
- **Key dimensions:** width, height, the hole pitch (58 × 49 full, 58 × 23 micro) and the 29 from the left hole column to the header centre.
- **Host board outline (approximate)**, on the Documentation Layer:
  - the Pi's 85 × 56 outline with R3 corners, from (0,0) to (85, 56);
  - its four Ø2.7 holes, at the same positions as the template's (they coincide);
  - the text "USB / Ethernet" at the X = 85 end.
  - The source is the Raspberry Pi 5 mechanical drawing, which the description names.
  - The group is gated `when: {size: [full]}`. The micro board's 58 × 23 hole pattern and its header row don't line up with this drawing, and no host drawing was decided for micro.

### Add-on board for Raspberry Pi Pico

This covers Pico and Pico 2, which share their mechanics.

| Parameter | Kind | Default | Range |
|---|---|---|---|
| `height` | length | 51 | 51 to 150 |

- **Outline:** 21 × `height`, with square corners because the Pico's corner radius isn't dimensioned. The USB end is at Y = `height`, so the board grows away from it.
- **Holes:** 4× `MountingHole_M2` at (4.8, `height` − 49), (16.2, `height` − 49), (4.8, `height` − 2) and (16.2, `height` − 2). At the default height that is Y = 2 and 49.
- **Connectors:** 2× `Socket_1x20` on the bottom, with columns 17.78 apart, matching the Pico pins:
  - J1 (Pico pins 1 to 20): pad 1 at (1.61, `height` − 1.37) → pad 20 at (1.61, `height` − 49.63).
  - J2 (Pico pins 21 to 40): pad 1 at (19.39, `height` − 49.63) → pad 20 at (19.39, `height` − 1.37).
- **Keepouts:** none.
- **Key dimensions:** width, height, the hole pitch (11.4 × 47) and the socket column spacing (17.78).

## New starter Parts

Each new Part has a Symbol, a Footprint and a Part file in the Starter Library. Each card-edge Footprint marks its fingers `role: edge_connector` ([edge connectors](edge-connectors.md#edge-connector-pads)).

### Headers and sockets

`Socket_2x20`, `Header_2x20`, `Socket_1x20`, `Header_1x20`, `Header_1x12`, `Header_1x16` and `Header_2x18`. They follow the existing 2.54 mm header and socket Parts in pads, courtyard and pin 1 marking.

### M.2 card edges

`CardEdge_M.2_Key_A`, `_B`, `_E`, `_M`, `_AE` and `_BM`.

- **Origin:** the centre of the finger edge. +X is toward pin 1.
- **Pin layout:** 75 positions, with pin *n* at X = 9.25 − (*n* − 1) × 0.25. Odd pins are on the top and even pins on the bottom.
- **Pads:** SMD, 0.35 wide.
  - Top pads run from Y = 0.50 to 2.00, and bottom pads from Y = 0.50 to 2.50.
  - The 0.50 setback is the most common open value.
- **Keys remove pins:** A 8–15, B 12–19, E 24–31 and M 59–66. A dual key removes both ranges.
- **Courtyard:** top side only, from the finger edge, and cut through at each key's 1.20 notch, so it stays on the board.
- **Symbol:** one pin per remaining pad, named by M.2 pin number.

### PCIe card edges

`CardEdge_PCIe_x1`, `_x4`, `_x8` and `_x16`, with 18, 32, 49 and 82 pins per side.

- **Origin:** the key centreline on the finger edge. +X is away from the bracket.
- **Pin layout:**
  - pins 1 to 11 at X = −11.50 + (*n* − 1);
  - pins 12 and up at X = 1.50 + (*n* − 12).
  - B-side pads are on top and A-side pads on the bottom.
- **Pads:** SMD, 0.70 wide, from Y = 1.40 to 5.60.
  - The open-library values are used because they are proven on open boards and clear of the 1.30 bevel. TE's Gen5 values are 0.60 / 2.60.
  - The short present-detect pins, A1 and that width's PRSNT2# pin (B17, B31, B48 or B81), run from Y = 2.40 to 5.60.
- **Courtyard:** top side only, from the finger edge, cut through at the 1.90 key notch and chamfered at the tab's two bottom corners, so it stays on the board. The Designator sits above the notch tip.
- **Symbol:** one pin per pad, named A*n* / B*n*.

### M.2 half-moon

`MountingHole_M2_Castellated` is a Mechanical Part with one Castellated Pad ([edge connectors](edge-connectors.md#castellated-pads)):

- a Ø3.5 plated hole;
- a Ø5.5 land on top and a Ø6.0 land on the bottom, which are the SATA-IO figures.

The Part's origin is the hole centre, and the template puts it on the far edge.

### Existing Parts

The existing `MountingHole_M2`, `MountingHole_M2.5` and `MountingHole_M3` Parts cover every other hole.
