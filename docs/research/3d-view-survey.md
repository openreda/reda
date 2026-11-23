# How other EDA tools show, navigate and select in a 3D board view

Research for #423, part of map #421 (v0.3 Mechanical & 3D). Gathered
2026-11-23; every source below was read on that date. The note records what
each tool does and where the tools differ. It recommends nothing for REDA.

Where a vendor's documentation could not be reached, or a page was silent on a
point, the text says so and the gap is listed again under
[Not verified](#not-verified). "Silent" means the pages read do not state it;
it does not mean the tool lacks the feature.

## Summary

- **Where the 3D view lives.** Three patterns. A *mode of the board editor's
  own canvas*: Altium Designer (keys `2` and `3`) and OrCAD X Presto (a 2D/3D
  mode switch in the Visibility panel). A *separate viewer window or page
  beside the 2D editor*: KiCad (`Alt+3`), Horizon EDA, DipTrace and EasyEDA
  Pro. A *separate linked document*: Fusion Electronics, where the 3D PCB is
  its own Fusion design that is synchronised with the 2D PCB.
- **Navigation.** Every tool has orbit, pan, zoom and six face-aligned view
  presets, but the mouse mapping differs in each: orbit is left-drag in KiCad,
  Horizon and DipTrace, `Shift`+right-drag on an on-screen sphere in Altium,
  and `Shift`+middle-drag in OrCAD X. Perspective and orthographic projection
  are both offered by Altium, KiCad, Horizon and DipTrace. 3D mice are
  supported by Altium (recommended in its system requirements), KiCad (vendor
  driver, Windows and macOS only) and Horizon (libspnav).
- **Show and hide.** KiCad and Altium give per-layer visibility, colour and
  opacity plus saved presets. Horizon has switches for solder mask,
  silkscreen, substrate, paste, copper and models. DipTrace offers colours and
  one "Display Components" switch. EasyEDA Pro hides layers and has a
  layer-spreading "Layer Expose". Fusion decides what geometry exists when the
  3D PCB is generated. Section planes were found only in Altium.
- **Selection and cross-probing.** KiCad highlights in 3D what is selected in
  2D, one way. Horizon selects in 2D what is clicked in 3D and shows a part
  info overlay. OrCAD X selects in 3D and cross-probes to 2D by a context-menu
  command. Altium shares one editor, so 2D and 3D are the same document view.
  DipTrace and EasyEDA Pro document no selection in 3D.
- **Measuring.** Altium has two 3D measuring commands (object or face to
  object, and vertex or snap point to snap point). OrCAD X shows the distance
  from the selected component to the one under the cursor while `Alt` is held.
  No 3D measuring tool is documented for KiCad, Horizon, DipTrace or
  EasyEDA Pro.
- **Rendering.** Real-time shaded rendering everywhere (DirectX 11 in Altium,
  OpenGL in KiCad and Horizon). KiCad alone adds a ray-traced mode. Altium has
  "Realistic" and "By Layer" colour schemes.
- **Editing in 3D.** Fusion and OrCAD X move components in 3D and send the
  move back to 2D. Altium positions 3D Bodies on a footprint or board with 3D
  picking tools. KiCad, Horizon, DipTrace and EasyEDA Pro place a model on a
  footprint by typed offset, rotation and scale next to a live 3D preview;
  Horizon adds point picking on the model, EasyEDA Pro an "Auto" fit.

## Comparison

| | Reached by | Orbit / pan / zoom | View presets | Projection | 3D mouse |
|---|---|---|---|---|---|
| Altium Designer | Mode of the PCB editor: `3` in, `2` out | `Shift`+right-drag on sphere / right-drag / `Ctrl`+wheel | `0`, `8`, `9`, numeric keypad | Ortho or perspective | Recommended by vendor |
| KiCad 10 | Separate window, `Alt+3` | Left-drag / middle-drag / wheel | `X` `Y` `Z` with `Shift`, navigator widget | Ortho toggle | Windows and macOS |
| Fusion Electronics | Separate linked document | Not read (Fusion Design workspace) | Not read | Not read | Not read |
| EasyEDA Pro | View > 3D Preview | Not documented | Six faces, fit, refresh | Not documented | Not documented |
| OrCAD X Presto | Mode switch in Visibility panel | `Shift`+middle-drag / not found / zoom commands | View cube | Not found | Not found |
| DipTrace 5.3 | Tools > 3D Preview > 3D Visualization | Left-drag (X, Y), middle-drag (Z) / right-drag / wheel | Side and isometric buttons | Ortho option | Not documented |
| Horizon EDA | Separate window, "3D" button | Left-drag / middle or `Shift`+left-drag / wheel | Six faces, rotate 90° | Ortho or perspective | libspnav |

| | Show / hide | Select and cross-probe | Measure | Edit in 3D |
|---|---|---|---|---|
| Altium Designer | Per-layer colour and transparency, 3D Bodies toggle, per-component opacity, section planes, single-layer mode | Same editor as 2D | Object or face distances; vertex and snap point distances | 3D Body placement tools |
| KiCad 10 | Appearance Manager: per-layer and per-type visibility, colour, opacity; presets; viewports | 2D selection highlighted in 3D | None documented | None; model offset typed in footprint dialog |
| Fusion Electronics | Chosen at generation: geometry to include, component list, presets | Not read | Not read | Move and flip components, holes, board outline |
| EasyEDA Pro | Hide layers, Layer Expose, board and plating colour | None documented | None documented | None; model calibrated in 3D Model Manager |
| OrCAD X Presto | Per-component and per-side visibility, per-object and global opacity | Click to select; right-click > Cross Probe to 2D | `Alt`+hover distance between components | Move component by typed X/Y; drag enclosure model |
| DipTrace 5.3 | Colours; Display Components | None documented | None documented | None; model shift, angle, scale typed |
| Horizon EDA | Switches for mask, silk, substrate, paste, copper, models; explode | Click in 3D selects in 2D; info overlay | None found | None on board; point-pick model placement in package editor |

## Altium Designer

**Reached by.** 3D is a display mode of the PCB editor, not a separate editor.
It is entered with View » 3D Layout Mode, the `3` key, or the 3D option in the
View Configuration panel, and left with `2`. Each mode keeps its own last view;
holding `Ctrl+Alt` while pressing `2` or `3` carries the current location and
orientation across instead.
[Controlling the 3D View](https://www.altium.com/documentation/altium-designer/pcb-3d-view)

**Navigation.** Zoom about the cursor with `Ctrl`+wheel, `Ctrl`+right-drag or
`PgUp`/`PgDn`. Pan with right-drag, the wheel (vertical) or `Shift`+wheel
(horizontal). Holding `Shift` shows a directional sphere at the cursor, which
sets the pivot; `Shift`+right-drag on the sphere's centre dot rotates freely,
on its horizontal arrow about Y, on its vertical arrow about X, and on its
circle segment about Z. Numeric keypad keys rotate in steps (30° by default)
and, with `Ctrl`, pan in steps. Presets: `0` from above, `9` from above rotated
90°, `8` "orthogonal", and keypad keys for top, left, front and isometric with
`Ctrl` giving the opposite side.
[Controlling the 3D View](https://www.altium.com/documentation/altium-designer/pcb-3d-view)

**Projection.** Orthographic or perspective, set in the View Configuration
panel. The same panel scales board thickness from 1x to 100x.
[Controlling the 3D View](https://www.altium.com/documentation/altium-designer/pcb-3d-view)

**3D mouse.** The system requirements list a "3D mouse for 3D PCB design, such
as the SpaceMouse" under recommended hardware.
[System Requirements](https://www.altium.com/documentation/altium-designer/system-requirements)

**Show and hide.** "Show 3D Bodies" toggles with `Shift+Z`. Colours are either
"Realistic" (from the selected configuration) or "By Layer" (the 2D layer
colours); layers have colour swatches and transparency sliders. Component
transparency is set per component, in 25% steps, in the PCB panel's 3D Models
mode. Custom view configurations save the 2D and the 3D display settings
together. Single-layer mode also works in 3D, stepping layers with
`Ctrl+Shift`+wheel. A Section View (behind the `PCB.SectionView` advanced
option) cuts the board with coloured section planes that are dragged by an origin
gizmo; it has Edit, On and Off states and an Invert Cutout switch.
[Controlling the 3D View](https://www.altium.com/documentation/altium-designer/pcb-3d-view),
[Your View of the PCB](https://www.altium.com/documentation/altium-designer/pcb-colors-visibility),
[Additional Tools for Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/additional-tools-3d-bodies)

**Selection and cross-probing.** Because 3D is a mode of the one editor, the
pages read describe no separate cross-probe step. They also do not describe
general click selection in 3D; see [Not verified](#not-verified).

**Measuring.** Two 3D commands. *Measure 3D Objects* (Reports menu) picks two
objects, or with `Ctrl` held two individual faces; hovering highlights the
candidate. Before the second pick it shows the first object's distance to the
board surface and board edge. It reports the shortest distance plus X and Y
distances, on the canvas and in the Messages panel; `Shift+C` clears results.
*Measure Distances* (Tools » 3D Body Placement) measures between two vertices,
reference points or custom snap points, on one body or two, and reports the
direct distance and the X, Y and Z deltas.
[Measuring Distances on a PCB](https://www.altium.com/documentation/altium-designer/measuring-distances-pcb)

**Rendering.** Real-time shaded. "Altium has chosen DirectX rather than OpenGL
for its graphics engine", and a DirectX 11 card is required.
[System Requirements](https://www.altium.com/documentation/altium-designer/system-requirements)
`Ctrl+C` in 3D copies a snapshot at a chosen resolution; PCB 3D Print, PCB 3D
Video and PDF 3D are output types.
[Controlling the 3D View](https://www.altium.com/documentation/altium-designer/pcb-3d-view)

**Editing in 3D: 3D Bodies.** A 3D Body is an object on a mechanical layer,
placed in a footprint in the PCB library editor or directly on a board. Types
are Generic (an imported STEP, Parasolid or SOLIDWORKS part, embedded or
linked), Extruded, Cylinder and Sphere. Properties include rotation about X, Y
and Z, standoff height, overall height, colour override and opacity. The 3D
Body Placement tools run in 3D mode and pick on the model: *Add Snap Points
From Vertices* (one vertex, or the midpoint of two), *Orient and Position 3D
Body* (three model points mated to three footprint points), *Position 3D Body*
(two-point translation), *Set Body Height*, and *Align Face With Board* (flat
faces only). The cursor snaps to vertices, hole centres and snap points. The
documentation says rotation is easier in 3D and X-Y alignment in 2D.
[Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb-3d-body),
[Additional Tools for Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/additional-tools-3d-bodies)

## KiCad

Read from the KiCad 10.0 PCB Editor manual; the 9.0 manual says the same
except where noted.
[KiCad 10.0 PCB Editor](https://docs.kicad.org/10.0/en/pcbnew/pcbnew.html),
[KiCad 9.0 PCB Editor](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)

**Reached by.** View → 3D Viewer or `Alt+3` opens the 3D Viewer, a separate
window from the PCB editor (the action is "Show 3D viewer window"). The
footprint editor has the same viewer, and the Footprint Properties dialog has a
smaller embedded 3D preview on its 3D Models tab.

**Navigation.** Left-drag orbits, by default about the board centroid;
pressing `Space` with the cursor over a point makes that point the pivot. The
wheel zooms, `Ctrl`+wheel pans left and right, `Shift`+wheel pans up and down,
and middle-drag pans. Default view hotkeys are `Z` top, `Shift+Z` bottom, `Y`
front, `Shift+Y` back, `X` right, `Shift+X` left, `Home` home view, `F` flip
board, `R` and `Shift+R` rotate about Z, and the arrow keys move the board.
KiCad 10 adds a "3D Navigator" widget of six clickable spheres for the six
standard views; the 9.0 manual does not have it. Viewports (camera positions)
can be saved and cycled with `Shift+Tab`.

**Projection.** A toolbar button switches orthographic projection on and off.

**3D mouse.** KiCad 7.0 added 3Dconnexion SpaceMouse support, contributed by
3Dconnexion, in the schematic, symbol, PCB and footprint editors and the 3D
viewer. It "only works on Windows and macOS, where 3dconnexion provides their
driver"; Linux support through libspacenav was being explored.
[KiCad 7.0.0 release post](https://www.kicad.org/blog/2023/02/Version-7.0.0-Released/)

**Show and hide.** The Appearance Manager panel sets visibility, colour and
opacity per board layer and per object type. Actions exist to show or hide
through-hole, SMD and other model types. Colours can follow the board stackup
or the PCB editor's copper colours. Appearance presets can be saved and cycled
with `Ctrl+Tab`; built-in presets include "Follow PCB Editor" and "Follow PCB
Plot Settings". Model bounding boxes and a 3D grid (1, 2.5, 5 or 10 mm) can be
shown. Components marked do-not-populate in the active variant are hidden.

**Selection and cross-probing.** One way: "selecting a footprint in the PCB
Editor will also highlight the component in the 3D Viewer". The manual
describes no selection made inside the 3D viewer.

**Measuring.** The 3D viewer's action list contains no measuring tool. The
measure tool listed in the manual belongs to the 2D editors.

**Rendering.** A real-time OpenGL renderer by default, plus a ray-tracing mode
that is "slower" and "more physically accurate"; the grid and selection
highlight are not drawn when ray tracing. Three material modes apply to models:
realistic materials, solid colours, and CAD colours. The view exports to an
image file or the clipboard.

**Editing in 3D.** None in the viewer. A model is attached to a footprint as a
file path (STEP, VRML or IDF), optionally embedded in the footprint. Offset,
scale and rotation are typed into fields beside the dialog's 3D preview, which
also has an opacity slider and a show checkbox per model.

## Fusion Electronics (successor to Eagle)

Autodesk's help pages are script-rendered; the static copies under
`help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/` were read. Several pages a
search engine still lists there return 404.

**Reached by.** The 3D PCB is "a Fusion design document that stores a
solid-model representation of your printed circuit board", viewed and edited
in Fusion's Design workspace. It is generated from the 2D PCB (Switch > 3D
PCB) and stays linked to it. It holds a board outline sketch, a board body
with the stackup thickness, 3D packages, and optionally "reference canvases":
flat images of copper and mask used in place of solid geometry.
[3D PCB](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-EDIT-3D-CPT.htm),
[Edit a 3D PCB inside its enclosure](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-EDIT-IN-PLACE-TSK.htm)

**Synchronising.** The 2D and 3D documents do not update live. After a change
on one side the other shows an Out-Of-Date icon, and clicking it pulls the
change. The help notes this replaced an earlier "push" mechanism.
[Synchronize the 2D PCB with mechanical assemblies](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-SYNC-2D-AND-3D-TSK.htm)

**Show and hide.** Decided when the 3D PCB is generated, in the 3D PCB
Settings dialog: presets Recommended, Fastest ("uses 2D images to represent all
objects except the board and components") and Most Detailed (full 3D
geometry); a list of geometry to include (solder masks, copper traces, vias,
polygons); a list of components to include, which also flags those with no 3D
model; and a silkscreen quality of low, medium or high. Parts without a 3D
package "appear as simple boxes".
[Synchronize the 2D PCB with mechanical assemblies](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-SYNC-2D-AND-3D-TSK.htm),
[3D PCB](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-EDIT-3D-CPT.htm)

**Editing in 3D.** The widest of the tools surveyed. *Move Component* moves a
component on the board by typed X and Y distances and an angle, or by drag
handles (arrows along an axis, a square for free movement, a circle for
rotation); its "Update PCB" option transfers the move to the 2D board and
"Reroute" keeps the connections attached. *Flip Component* moves a component
to the other side. Holes and the board outline can be changed while
referencing the enclosure's geometry, and a board can be created from a sketch
or a face of a mechanical design.
[Move a 3D component](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-MOVE-3D-TSK.htm),
[Integrate a PCB with mechanical assemblies](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-3D-BOARD-CPT.htm),
[Create a PCB from a mechanical design](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-PCB-MECH-DESIGN-CPT.htm)

**Navigation, selection, measuring, rendering.** These belong to Fusion's
general Design workspace, not to the electronics help, and were not read.

## EasyEDA Pro

**Reached by.** View > 3D Preview, or a toolbar icon. A second function, 3D
Shell Preview, previews the enclosure ("3D shell") drawn in the PCB editor and
stays in step with it while the shell is drawn. The pages do not say whether
either opens as a tab, a panel or a window.
[3D Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d/),
[3D Shell Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d-shell/)

**Navigation.** The page lists preset views (top, bottom, left, right, front,
back), "Adapt to all" (fit), "Refresh" (back to the default angle), and an
"Import change" button that brings in PCB changes, which implies the preview
is not live. Mouse controls are not documented.
[3D Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d/)

**Show and hide.** Three display styles: normal, contour (for 3D shells) and
Gerber view. The property panel sets board material, silkscreen technology,
background colour, board colour (seven choices) and pad plating colour (gold
or silver). Layers and the 3D shell can be hidden. "Layer Expose" spreads the
layers apart so each can be seen; an "Explosion" control separates the shell
halves.
[3D Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d/)

**Selection, measuring.** Not documented on the pages read.

**Rendering and export.** The toolbar exports a picture (default width 2160,
transparent background) and 3D models. The page warns the preview is a
simulation and not a production reference.
[3D Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d/)

**Editing in 3D.** The 3D Shell Preview can write "PCB height from the bottom
of the casing" back to the PCB. Models are bound per device in the 3D Model
Manager (Tools menu), which shows the footprint alone and the footprint with
the model; a calibration area sets the model's position and angle relative to
the footprint, and an "Auto" button fits the model's size and position from
its top view, after which "it still needs to be fine-tuned manually".
[3D Shell Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d-shell/),
[3D Model Manager](https://prodocs.easyeda.com/en/pcb/tools-3d-model-manager/)

## OrCAD X Presto

Cadence's own product documentation is behind a login, and its public
resource pages returned no article text to the fetcher. Everything below
rests on one how-to for OrCAD X Presto 23.1 by EMA Design Automation (dated
2024-09-19; the page states EMA is now part of Cadence). Treat it as a single
secondary source.
[How to Use 3D Functionality in OrCAD X](https://www.ema-eda.com/how-to-page/how-to-use-3d-functionality-in-orcad-x/)

**Reached by.** In the Visibility panel, a display mode button offers 2D and
3D; "Select 3D for the Mode to open the 3D viewer". Switching back to 2D is
the same control.

**Navigation.** "The 3D view can be rotated by holding the middle mouse button
and SHIFT while moving the mouse." A cube at the bottom-left of the canvas
gives preset views such as Top. Zoom In is on the Zoom Fit icon's context
menu. Panning and wheel zoom are not described.

**Show and hide.** The Visibility panel lists components per board side, each
with an eye icon, and a side-level eye for all of them. Mechanical models such
as an enclosure STEP file are added in the same panel and have their own eye
and opaque/transparent icons. A global opacity slider and per-object opacity
exist, including for placement boundaries and 3D models.

**Selection and cross-probing.** Clicking a component in 3D selects it.
Right-click > Cross Probe marks it so that it is highlighted after switching
the mode to 2D. "Show in Search Results" lists it in the Search panel.

**Measuring.** With a component selected, holding `Alt` and hovering over
another shows the distance between the two.

**Editing in 3D.** A selected component is moved by typing X and Y in the
Properties panel. An enclosure model is positioned by dragging or by typed
offsets and rotations. "Collision checking can be performed by configuring 3D
clearance rules and performing design rule checks."

## DipTrace

Read from the DipTrace complete help for version 5.3.
[DipTrace complete help](https://diptrace.com/support/tutorials/help_html/)
(sections 3.13 and 5.3.11),
[3D modeling](https://diptrace.com/diptrace-software/3d-modeling/)

**Reached by.** Tools \ 3D Preview \ 3D Visualization, or a toolbar button, in
PCB Layout. The help calls it the "3D Preview & Export module". Before it
opens, a dialog lists the components and the models found for them, because
DipTrace searches its model folders for a model matching each pattern that has
none. The preview does not follow edits live: a Redraw button applies changes.

**Navigation.** "Use the left mouse button to rotate the circuit board model
around X or Y axes, press the mouse wheel (middle button) to rotate around the
Z axis, hold the right mouse button to pan, and scroll the mouse wheel to Zoom
In/Out." Side and isometric view buttons and a Reset View button exist. An
"Orthographic (no perspective)" option switches projection.

**Show and hide.** Colours of the board, silkscreen, solder mask, copper and
background, and one "Display Components" checkbox. Board thickness comes from
the layer stackup or a typed value. The help notes the solder mask is drawn
transparent over the board colour.

**Selection, measuring.** Not documented.

**Editing in 3D.** None in the board preview. A model is attached in the
Pattern Editor or from a component's context menu in PCB Layout, in a dialog
with its own 3D preview using the same mouse controls. Three sources: a model
file (3DS, VRML, STEP or IGES); an IPC-7351 model generator, available when
the pattern was made by the pattern generator; and a shape extruded from a
component outline to a typed height. "DipTrace automatically places a 3D model
to fit the pattern's drawing"; shift, rotation and scale per axis are typed
and apply at once.

**Export.** STEP and VRML 2.0 from the preview. "STEP model does not include
the copper traces"; VRML can.

## Horizon EDA

Horizon's documentation says only that the 3D view lets you "look at your
board as if you were holding it in your hands" and that packages take STEP
models. The detail below is read from the source at commit `79e3422`
(2026-11-17).
[Feature overview](https://docs.horizon-eda.org/en/stable/feature-overview.html),
[3d_view.cpp](https://github.com/horizon-eda/horizon/blob/79e3422646262e982ab6d2b870857490270fc875/src/imp/3d/3d_view.cpp),
[3d_view.ui](https://github.com/horizon-eda/horizon/blob/79e3422646262e982ab6d2b870857490270fc875/src/imp/3d/3d_view.ui),
[canvas3d.cpp](https://github.com/horizon-eda/horizon/blob/79e3422646262e982ab6d2b870857490270fc875/src/canvas3d/canvas3d.cpp),
[imp_board.cpp](https://github.com/horizon-eda/horizon/blob/79e3422646262e982ab6d2b870857490270fc875/src/imp/imp_board.cpp)

**Reached by.** A "3D" button in the header of the board editor and of the
package editor presents a separate window titled "3D Preview". `Esc` in that
window brings the 2D editor window back to the front. The board is not
rebuilt live: an Update button rebuilds it, and progress labels read "Loading
3D models…" and "Rendering layers…".

**Navigation.** Left-drag rotates (azimuth and elevation). Middle-drag or
`Shift`+left-drag pans. The wheel zooms. With a touchpad or trackpoint the
scroll gesture pans, `Ctrl`+scroll zooms and `Shift`+scroll rotates. Touch
gestures pan, pinch-zoom and rotate. Buttons give Front, Back, Top, Bottom,
Right and Left, rotation by 90° left and right, and view all; view changes are
animated. An axes indicator shows orientation.

**Projection.** Perspective or orthographic, with actions for each and a
toggle.

**3D mouse.** Built against libspnav. Motion events pan, zoom and rotate with
scale and threshold preferences, the device's buttons can be bound to actions,
and the window shows "Spacenav input is available" when a device is found.

**Show and hide.** Switches for solder mask, silkscreen, substrate and solder
paste, with colour buttons for mask, silkscreen and substrate (the colours are
stored with the board). Copper is On, Off or "Layer colors". 3D models are
All, Placed or None. A "Height restrictions" switch and an "Explode" slider
exist. Background colour and antialiasing level are settings.

**Selection and cross-probing.** A click in the 3D view (a press and release
within 50 pixels) picks the package under the cursor. On a board the package
is highlighted in 3D, selected in the 2D canvas when no tool is active, and an
overlay shows its reference designator, value, MPN, manufacturer, package name,
description and datasheet link. Selecting parts in the board editor's Parts
window highlights them in 3D.

**Measuring.** No measuring command was found in the 3D view's source.

**Rendering.** OpenGL, real-time, with multisample antialiasing. The source
tree also has a 3D image exporter, exposed in the Python module.

**Editing in 3D.** None on a board. In the package editor the 3D window is
used to place the STEP model: a "place model" box offers Pick (one point on
the model) and "Pick two (center)" (the midpoint of two), with the model's
points shown for picking, and moves that point to a pad or to the origin.

## Where the tools differ

- **Mode, window or document.** Altium and OrCAD X switch the editing canvas
  itself between 2D and 3D. KiCad and Horizon open a second window that sits
  beside the 2D editor. DipTrace and EasyEDA Pro open a preview that is
  refreshed on demand. Fusion makes the 3D board a separate saved document.
- **Live or refreshed.** Altium's 3D is the same document view. KiCad has a
  "Reload board" action, Horizon an Update button, DipTrace a Redraw button,
  EasyEDA Pro "Import change", and Fusion an Out-Of-Date icon to click.
- **Orbit gesture.** Left-drag (KiCad, Horizon, DipTrace) against a modified
  right- or middle-drag (Altium, OrCAD X). Only Altium and KiCad document a
  user-set pivot: Altium at the cursor when `Shift` is pressed, KiCad at the
  cursor when `Space` is pressed.
- **Direction of cross-probing.** KiCad: 2D to 3D only. Horizon: 3D to 2D by
  click, and Parts window to 3D. OrCAD X: 3D to 2D by command.
- **What 3D editing means.** Moving components (Fusion, OrCAD X), positioning
  a model on its footprint by picking geometry (Altium, Horizon), or nothing
  beyond typed model offsets (KiCad, DipTrace, EasyEDA Pro).
- **Measuring.** Present with face, vertex and snap-point picking in Altium;
  a component-to-component readout in OrCAD X; absent from the other tools'
  documentation.
- **Rendering.** Ray tracing only in KiCad. A layer-colour 3D scheme in
  Altium and (for copper) in Horizon and KiCad. Exploded or spread layers in
  Horizon and EasyEDA Pro. Section planes only in Altium.
- **Footprint editor.** KiCad and DipTrace embed a 3D preview in the footprint
  properties or model dialog. Horizon opens its 3D window from the package
  editor. Altium's PCB library editor uses the same 3D mode and 3D Body tools
  as the board editor. EasyEDA Pro binds models from the board, per device.

## Not verified

- **Altium, general selection and editing in 3D.** The current pages read do
  not say whether components, tracks or other objects can be selected, moved
  or routed in 3D Layout Mode, beyond the 3D Body tools. Older TechDocs pages
  that a search engine summarises as saying 3D mode lacks the full editing
  range now redirect to the current documentation and could not be read.
- **Altium, split 2D and 3D views of one board, and cross-selection between
  them.** Not found in the pages read.
- **Altium, the `Ctrl+F` / `V`,`B` flip-board shortcuts and the full 3D
  shortcut table.** The PCB editor shortcuts page returned 404.
- **Altium, which 3D Body tools work in the PCB library editor.** The pages
  say bodies are placed in footprints there, but do not state per tool where
  it runs.
- **Fusion, navigation, selection, measuring and rendering in the 3D PCB.**
  These are general Fusion Design workspace behaviour and were not read. The
  "About the 3D circuit board view" and "Edit the 3D circuit board" pages
  returned 404.
- **EasyEDA Pro, mouse navigation, selection, measuring, model file formats,
  and whether the preview is a tab or a window.** The documentation is silent.
- **OrCAD X.** No first-party Cadence documentation was reached. Panning,
  projection, 3D mouse support, cutting planes, the "3D to 2D move" command
  and collision detection as named in Cadence marketing snippets are
  unverified; so is whether the EMA how-to still matches current releases.
- **DipTrace, the rendering API, and whether the preview is a separate
  window.** Not stated in the help.
- **Horizon, whether a 2D canvas selection highlights in 3D.** Only the Parts
  window path and the 3D-to-2D path were seen in the source read; the rest of
  the board editor's source was not searched exhaustively.
- **KiCad, measuring in 3D.** Absence is inferred from the manual's action
  list for the 3D viewer, not from a statement.
