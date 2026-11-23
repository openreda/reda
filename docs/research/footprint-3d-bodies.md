# How EDA tools store and attach 3D bodies and models on footprints

Research for [#424](https://github.com/openreda/reda/issues/424), part of the v0.3 map [#421](https://github.com/openreda/reda/issues/421).
All sources were read on 2026-11-23. This note records what each tool does and where the tools differ. It recommends nothing for REDA.

Source quality differs per tool, and each section says what it rests on:

- **KiCad and Horizon EDA:** official documentation plus source code.
- **Altium Designer, Fusion, EasyEDA Pro, DipTrace:** the vendor's own documentation.
- **OrCAD X:** Cadence's own reference documentation could not be reached. That section rests on training pages from EMA Design Automation, a Cadence reseller, and is marked accordingly.

## Summary table

| | What a body is | Model formats | Stored or linked | Placement on the footprint | Several per footprint | Fallback with no model |
|---|---|---|---|---|---|---|
| **Altium Designer** | A 3D Body object on a mechanical layer: Generic (wraps a model), Extruded, Cylinder or Sphere | STEP, Parasolid, SOLIDWORKS part | Embedded in the library or board file; linking by folder exists for boards but not in the library editor; Workspace ("Server") models | X/Y location, rotation about X/Y/Z, standoff height, snap points, align-face-with-board | Yes: the documented TO-92 example uses one body for the case and separate bodies for the pins | Footprint `Height` property and bounding box are used by the rules |
| **KiCad 9 / 10** | A list of references to model files | STEP, IGES, VRML, X3D, IDF | Linked by path with path variables, or embedded in the footprint file | Offset, rotation and scale per axis, plus opacity and a show flag | Yes | Nothing is drawn; the footprint keeps the path |
| **KiCad master (unreleased)** | Adds an extruded body beside the model list | as above | Stored as parameters in the footprint | Overall height, standoff, outline source layer, offset, rotation, scale, material, colour | Models: yes. Extruded body: one, by a stated current restriction | The extruded body is the fallback, and is exported to STEP |
| **Fusion Electronics** | A "3D package": a Fusion solid design paired with a footprint | Fusion designs; STEP upload | Saved as a package in the library | Align Package: face on the footprint plane, offset, flip, rotate, in-plane alignment | Not stated in the pages read | Not stated in the pages read |
| **EasyEDA Pro** | A 3D model library entry, bound by UUID | STEP, OBJ, WRL (zip for batches) | Stored in the user's library; the footprint or device holds its UUID | Target size, rotation Z/X/Y, offset X/Y/Z | The format has one `3D Model` attribute | Export builds a box from the outline and the `Height` property |
| **OrCAD X Presto** (reseller source) | A STEP model mapped to the footprint | STEP | Mapping saved in the footprint `.dra` | Manual drag, snap to top or bottom, snap model pin to pad, numeric offset and rotation | Not established | Place-bound shape with a height (forum source only) |
| **DipTrace** | One model per pattern: a model file, a generated IPC-7351 model, or an extruded outline | 3DS, VRML, STEP, IGES | Model folders searched by pattern name; the note could not establish what is written into the library | Shift, rotation angle and scale per axis; automatic initial fit | Not established | Only the footprint is shown; a search by pattern name runs when the 3D preview opens |
| **Horizon EDA** | A list of STEP models on the package, one marked default | STEP only | File inside the pool directory, referenced by pool-relative path | x, y, z shift and roll, pitch, yaw; align and project tools | Yes, as alternatives: a board package selects one | Height rule warns "has no 3D model" |

## Altium Designer

### What a body is

A 3D Body is "a primitive design object that is used as a container into which a standard-format generic 3D model can be imported", and it is optional on a footprint.
There are four kinds: Generic (it holds a model and resizes itself to enclose it), Extruded, Cylinder and Sphere.
([Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb/3d-bodies))

- **Extruded:** a polygon drawn in the X-Y plane and extruded in Z. It has an Overall Height (its top) and a Standoff Height (its underside), and may carry a texture image on its top surface.
- **Cylinder:** Height, Radius and standoff.
- **Sphere:** Radius and standoff.
- **Rotation limits:** "The Extruded and Cylinder 3D Body shapes cannot be rotated around all three axes." An extruded body rotates only about Z, a cylinder about Y and Z. Convert To STEP turns a simple body into an embedded model, which can then rotate freely but can no longer be resized.
- **Display:** each body has a colour and an opacity.

([Working with Extruded, Spherical & Cylindrical 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb/3d-bodies/extruded-spherical-cylindrical))

Bodies sit on a chosen mechanical layer. If that layer is half of a component layer pair, the body moves to the paired layer when the component is flipped to the other side of the board. (Same two pages.)

### Formats

Generic bodies accept STEP (`*.stp`, `*.step`), SOLIDWORKS parts (`*.SldPrt`) and Parasolid (`*.x_t`, `*.x_b`).
([Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb/3d-bodies))

### Embedded, linked or from a Workspace

- **Embed Model** stores the model inside the PCB library or PCB file.
- **Link to Model** references an external file. The file must sit in a folder registered on the PCB Editor - Models preferences page, and only that folder is searched, not its subfolders. Linking is not available in the PCB library editor. When a linked file has changed, the software warns as the board opens so the model can be refreshed.
- **Server** takes the model from the connected Workspace.

([Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb/3d-bodies))

**Missing models.** For a Server-sourced body with no valid model, opening the board reports "Can't load model (ID = {...})", once per placed instance, and the message does not name the model. Altium's fix is to assign a valid Workspace model or embed the file.
([KB: Error "Can't load model"](https://www.altium.com/documentation/knowledge-base/altium-designer/error-cant-load-model-when-opening-a-pcb-document))
What is drawn when a folder-linked file is missing was not found in the documentation.

### Positioning

- **Properties:** X and Y location, rotation about X, Y and Z, and Standoff Height, "the distance from the board surface to the underside" of the model. A negative standoff lets the body pass through the board.
- **Snap points:** a body carries a list of snap points (Add, Average, Delete). They are created in 3D with Tools » 3D Body Placement » Add Snap Points From Vertices, where a midpoint mode puts a point between two picked vertices, and they can then be used in 2D to drag the body onto a pad hole.
- **Face alignment:** Tools » 3D Body Placement » Align Face With Board seats a picked face of the model on the board surface.

([Working with 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb/3d-bodies); [KB: Position 3D Body model precisely in PCB footprint library](https://www.altium.com/documentation/knowledge-base/altium-designer/position-3d-body-model-precisely-in-pcb-footprint-library))

So positioning happens in a 3D view of the footprint editor, combined with the 2D view.

### Generating bodies

- **Component Body Manager** (Tools » Manage 3D Bodies for Current Component) builds extruded bodies from existing footprint geometry: a bounding rectangle, or closed shapes traced along the centreline of tracks and arcs on chosen layers. Overall Height defaults to the footprint's Height property. The documented TO-92 example takes the body from the silkscreen outline and the pins from squares on a mechanical layer.
  ([Extruded, Spherical & Cylindrical 3D Bodies](https://www.altium.com/documentation/altium-designer/pcb/3d-bodies/extruded-spherical-cylindrical))
- **IPC Compliant Footprint Wizard** takes "Courtyard, Assembly, and Component (3D) Body Information" among its dimensions, previews a STEP model, and with "Produce 3D/STEP model" enabled writes the model either Embedded (the default) or as an External File (STEP, or Parasolid with a CoDesigner licence). The page lists about thirty package families, from BGA and chip parts to SOIC, SOT, QFN and QFP.
  ([Creating a PCB Footprint](https://www.altium.com/documentation/altium-designer/components-libraries/creating-pcb-footprint))

### What the bodies are used for

- **Height.** The footprint has a Height property, which that page calls the "simplest level of 3D representation". The Height rule has Minimum, Preferred and Maximum values and checks the overall height of the 3D Body if there is one, otherwise the Height property. It can be scoped to a component class or to a Room.
- **Component Clearance.** The rule has a Minimum Horizontal Clearance and a Vertical Clearance Mode: Infinite (anything above or below is a violation) or Specified (the exact shapes are compared and a Minimum Vertical Clearance applies). Options include "Do not check components without 3D body" and "Check clearance by component boundary". With no body, the footprint's bounding box gives the X-Y area and the Height property gives Z. The page states: "The Component Clearance rule does not check for clearance violations between 3D bodies and the board surface."

  ([Placement Rule Types](https://www.altium.com/documentation/altium-designer/pcb-placement-rule-component-clearance), page dated 2026-11-01)
- **Export.** STEP export has a "3D Bodies Export Options" choice for components that have both simple bodies and models: prefer the simple bodies, prefer the models, or export both. "If only extruded 3D bodies are available for components, they will always be exported." Parasolid, VRML and IDF export also exist.
  ([STEP Export-Import Support](https://www.altium.com/documentation/altium-designer/step-export-import?version=21), a version 21 page)

## KiCad

### Released versions (9.0 and 10.0)

**What a body is.** A footprint holds a list of 3D models. Each is a file reference with a transform. The documentation says: "3D models are normally external files that footprints can link to, but they can optionally be embedded in footprints."
([PCB editor documentation source, 9.0](https://gitlab.com/kicad/services/kicad-doc/-/blob/9.0/src/pcbnew/pcbnew_footprints_and_libraries.adoc))
There is no extruded or primitive body in 9.0 or 10.0: `pcbnew/pcb_3d_body.h` does not exist on the 10.0 branch.

**Stored fields.** Per model: filename, show flag, opacity, offset (mm), scale (factor) and rotation (degrees), each a three-axis vector.
([`FP_3DMODEL` in footprint.h, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/pcbnew/footprint.h); [writer in pcb_io_kicad_sexpr.cpp, 9.0](https://gitlab.com/kicad/code/kicad/-/blob/9.0/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp))
The writer emits `(model "file" [hide] [(opacity n)] (offset (xyz …)) (scale (xyz …)) (rotate (xyz …)))`. The published [file-format page](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/index.html) still shows `(at (xyz …))` and omits `hide`, `opacity` and `offset`, so it lags the source.

**Formats.** The documentation names STEP, VRML and IDF. The loader plugins accept more: `stp`, `step`, `stpz`, `stp.gz`, `step.gz`, `igs`, `iges` (OpenCASCADE plugin), `wrl`, `wrz`, `x3d` (VRML plugin) and `idf`, `emn` (IDF plugin).
([oce.cpp](https://gitlab.com/kicad/code/kicad/-/blob/9.0/plugins/3d/oce/oce.cpp), [vrml.cpp](https://gitlab.com/kicad/code/kicad/-/blob/9.0/plugins/3d/vrml/vrml.cpp), [s3d_plugin_idf.cpp](https://gitlab.com/kicad/code/kicad/-/blob/9.0/plugins/3d/idf/s3d_plugin_idf.cpp))

**Linked or embedded.** Paths may contain path variables such as `${KIPRJMOD}` or `${KICAD9_3DMODEL_DIR}`; older versioned variables resolve to the current one. Since 9.0 a file can be embedded in the footprint: ticking Embed File in the file browser stores a copy in the footprint and uses the embedded reference as the path. Embedded files are deduplicated when the footprint is placed many times on a board.
(Documentation source above.)

**Positioning.** "By default, models are added with their origin placed at the footprint's origin, with no offset, scaling, or rotation." Offset, scale and rotation are typed into the 3D Models tab of Footprint Properties, beside a live 3D preview of the footprint with its models. There are no snap points or face-alignment tools in that description. Each model also has an opacity slider and a show checkbox.

**Several per footprint.** Yes: "Each added model will be shown in the footprint preview as well as in the full PCB 3D view".

**Missing files.** In the dialog, "an icon in the leftmost column will indicate an error". In the viewer, "The 3D model for a component will only appear if the 3D model file exists". A missing file is expected in the standard library: footprints "may contain a path to a 3D model that does not yet exist, in anticipation of the 3D model being created in the future."
([inspecting documentation source, 9.0](https://gitlab.com/kicad/services/kicad-doc/-/blob/9.0/src/pcbnew/pcbnew_inspecting.adoc))
The viewer has a Show Model Bounding Boxes preference (same source; also present in the 10.0 documentation).

**Library conventions.** The KiCad Library Conventions require:

- every non-virtual footprint to reference a model, "even if the 3D model is missing";
- the model to live in a `.3dshapes` directory named after the footprint library and to share the footprint's name;
- variants that look the same to share one model;
- scale 1:1:1, rotation and offset zero, so alignment is done in the modelling tool and not in the footprint;
- the `${KICAD9_3DMODEL_DIR}/` path prefix and a `.step` file.

([F9.3](https://gitlab.com/kicad/libraries/klc/-/blob/master/content/footprint/F9/F9.3.adoc), [M2.1](https://gitlab.com/kicad/libraries/klc/-/blob/master/content/model/M2/M2.1.adoc), [M2.2](https://gitlab.com/kicad/libraries/klc/-/blob/master/content/model/M2/M2.2.adoc). The rendered site klc.kicad.org refused automated fetches, so the rule sources were read instead.)

The 9.0 documentation says the opposite about file type: "footprints in KiCad's library only reference the VRML versions of the models". The two sources disagree, and which one describes the current library was not checked against the library files.

**How standard-package models are generated.** The official model library holds STEP models "for rendering and MCAD integration". Models come from scripts or from manual modelling, with FreeCAD preferred.
([kicad-packages3D README](https://gitlab.com/kicad/libraries/kicad-packages3D/-/blob/master/README.md))
The generator scripts are Python on CadQuery 2 and OpenCASCADE 7, and produce "STEP AP214 3D models and VRML 3D models". Their repository is archived, and the scripts moved into the footprint generator repository "because the footprints and 3D models are closely related by the physical dimensions of the parts."
([kicad-packages3D-generator README](https://gitlab.com/kicad/libraries/kicad-packages3D-generator/-/blob/master/README.md))
Contributed models must be the contributor's own work; files downloaded from a manufacturer's site are to be avoided.
([M1.1](https://gitlab.com/kicad/libraries/klc/-/blob/master/content/model/M1/M1.1.adoc))
The libraries are CC-BY-SA 4.0 with an exception for designs that use them; redistributing the library as a collection keeps the share-alike terms.
([LICENSE.md](https://gitlab.com/kicad/libraries/kicad-packages3D/-/blob/master/LICENSE.md))

**What the models are used for.**

- **Drawing:** the 3D viewer and the footprint preview.
- **Export:** the 3D exporter writes STEP, GLB, BREP, XAO, PLY and STL from the board "and any STEP files specified in footprints". VRML models cannot go into STEP, BREP or XAO output; with "Substitute similarly named models" the exporter looks for a STEP file of the same name. Components can be filtered by selection, reference pattern, DNP and footprint type. VRML and IDF have separate exporters; IDF export uses only IDF component outlines attached to footprints.
  ([generating outputs documentation source, 9.0](https://gitlab.com/kicad/services/kicad-doc/-/blob/9.0/src/pcbnew/pcbnew_generating_outputs.adoc))
  On a missing file the STEP exporter reports "Could not add 3D model for %s. File not found: %s" and carries on.
  ([exporter_step.cpp, master](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/exporters/step/exporter_step.cpp))
- **Checks:** none use the models. Part-to-part spacing is a 2D check on Courtyards ("Courtyards overlap", the `courtyard_clearance` constraint). The documented DRC violations and custom-rule constraints for 9.0 contain no component height rule and no 3D collision check.
  ([inspecting](https://gitlab.com/kicad/services/kicad-doc/-/blob/9.0/src/pcbnew/pcbnew_inspecting.adoc) and [advanced](https://gitlab.com/kicad/services/kicad-doc/-/blob/9.0/src/pcbnew/pcbnew_advanced.adoc) documentation sources)

### Development branch (master, unreleased)

The master branch, read at its state on 2026-11-23, adds an extruded body. This is unreleased code and may change before it ships.

- **Model.** A footprint has `Bodies()`, a list of `PCB_3D_BODY` items, commented "3D bodies (extruded or model)". A body has a kind: `EXTRUDED`, or `MODEL` which is "reserved; not constructible yet". The file-model list (`FP_3DMODEL`) still exists beside it. Accessors carry the comment "Convenience accessors for now while we restrict to only one body per footprint". The body became its own object in a commit of 2026-10-17, "Extract 3D body to first-class object".
  ([footprint.h](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/footprint.h), [pcb_3d_body.h](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_3d_body.h))
- **Fields.** Height, standoff, outline source layer, colour, material (plastic, matte, metal, copper), show flag, and scale, rotation and offset vectors.
- **Outline source.** The body stores no shape of its own. Its "layer" names where the outline comes from: Auto, Courtyard layer, Fabrication layer, Silkscreen layer or Pin bounding box. A placeholder box is computed from pads first, then graphics on fabrication, courtyard and silkscreen layers, then the footprint bounding box. Through-hole pads get extruded pins, sized to about 90% of the drill.
  ([panel_fp_properties_3d_model.cpp](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/dialogs/panel_fp_properties_3d_model.cpp), [3d_placeholder_utils.h](https://gitlab.com/kicad/code/kicad/-/blob/master/3d-viewer/3d_rendering/3d_placeholder_utils.h))
- **File format.** It reuses the `model` token: `(model (type extruded) (uuid …) (hide …) (overall_height …) (body_pcb_gap …) (layer auto | pad_bbox | "<layer>") (material …) (color …) (offset …) (scale …) (rotate …))`. A body with zero height is not written.
  ([pcb_io_kicad_sexpr.cpp, master](https://gitlab.com/kicad/code/kicad/-/blob/master/pcbnew/pcb_io/kicad_sexpr/pcb_io_kicad_sexpr.cpp))
- **Footprint height.** The footprint also gains a `height` of its own, "independent of any individual 3D body / 3D model on a footprint. It is used for IDF export when there isn't an explicit extruded body configured, and can also be used for custom rules and perhaps more things in the future."
- **Export.** The STEP exporter adds the extruded body and its pins, and the 3D Models panel can export one extruded body to STEP, GLB, STL or BREP.

## Fusion Electronics (and Eagle)

Read from Autodesk's Fusion help. Eagle's own 3D package handling was not researched separately.

- **What a body is.** A library holds footprints and "packages" (3D packages). A package is a Fusion solid model paired with a footprint; a device then ties symbol, footprint and package together. Models are added "by creating new models over existing footprints" or by "generating new 3D models and footprint pairs from specified IPC standards".
  ([Adding 3D models to electronic libraries](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-ATTACH-3D-MODELS-CPT.htm), [Finish creating a 3D package](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-FINISH-3D-PKG-TSK.htm))
- **Sources of a model.**
  - Draw it with Fusion's solid modelling tools in the package editor. ([Create a package](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-CREATE-PACKAGE-TSK.htm))
  - Attach an existing Fusion design: right-click a footprint, Attach Existing 3D Model; "Fusion builds the package from the selected design and the footprint outline". ([Create a package from an existing design](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECAD-ATTACH-EXISTING-PKG-TSK.htm))
  - Upload a STEP file through the same command with "Select from my computer…". ([Create a package from a local STEP file](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECAD-UPLOAD-FROM-STEP-TSK.htm))
  - Generate one (below).
- **Generation.** The Package Generator offers an alphabetical list of sample packages (the page's example is "SOIC, SOP"), then a specification panel with tabs for package, footprint, thermal and manufacturing details. Update redraws the 3D component with the pad outlines. Finish saves "the package and its related footprint" to the library under a generated name. The generator therefore makes footprint and model together from one set of dimensions.
  ([Generate a 3D package](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECD-GENERATE-3D-PKG-TSK.htm))
- **Positioning.** Align Package takes a planar face or circular edge of the model that sits on the footprint plane and aligns it automatically, assuming the package is symmetric. It then offers an Offset above the plane, Flip (180°), Rotate (90° in plane), and an in-plane mode: Free Move, Center (between two parallel footprint edges) or Point to Point (a point on the package to a point on the footprint).
  ([Align a 3D package over its footprint](https://help.autodesk.com/cloudhelp/ENU/Fusion-ECAD/files/ECAD-ALIGN-3D-PKG-TSK.htm))
- **Not established from the pages read:** whether one footprint can carry several packages, what is shown when a package is missing, and any height or collision rule in the board editor. Board-level 3D in Fusion is a full mechanical design, where interference tools are Fusion's own; that was not verified here.

## EasyEDA Pro

- **What a body is.** A 3D model is its own library item, created with File > New > 3D Model. Accepted files are zip, STEP, STP, WRL and OBJ; an OBJ and its MTL go in one zip, and a zip of several files is a batch import. The import asks for the model's unit (mm by default; cm, m, mil or inch). The model lands in the user's personal library.
  ([Create 3D Library](https://prodocs.easyeda.com/en/3d-model/file-new-3d-model/))
- **Attachment.** The footprint or device carries two attributes: `3D Model`, the model's UUID (a library record, not a path), and `3D Model Transform`.
- **Transform.** Nine comma-separated values: `sizeX, sizeY, sizeZ, rotZ, rotX, rotY, offX, offY, offZ`, describing the model with the part on the top layer at 0,0 and rotation 0. For `sizeZ`, "if 0, height is automatically adapted". The matrix is `OFFSET × ROT × SCALE × ORIGIN`, where ORIGIN moves the model's X-Y centre and lowest Z to zero, and SCALE is the target size divided by the model's own extent. So the model is always normalised to its bounding box and then stretched to a stated size, whatever origin and unit the file had.
  ([Footprint System format](https://prodocs.easyeda.com/en/format/pcb/component/))
- **Binding UI.** The 3D Model Manager lists the board's devices and binds a model to the selected one, with a calibration area for position and angle. An Auto button fits the model's top view to the footprint's size, and "After clicking Auto, it still needs to be fine-tuned manually."
  ([3D Model Manager](https://prodocs.easyeda.com/en/pcb/tools-3d-model-manager/))
- **Several per footprint.** The format page describes a single `3D Model` attribute. No way to bind more than one was found.
- **Missing models and export.** Export writes STEP or OBJ. STEP export needs a bound model that is itself STEP, "Otherwise, the exported STEP will not have a model." The export reports each failure: unbound model, bound model not in STEP format, bound model ID not found, or a network failure while loading. "For components without bound 3D models, 3D models will be automatically generated (based on the "Height" property)", as a box on the footprint's rectangular shape.
  ([Export 3D file](https://prodocs.easyeda.com/en/pcb/export-3d-file/))
- **Other uses.** The 3D preview can show the board inside a 3D shell (enclosure). The preview page describes no interference check and no measuring tool.
  ([3D Preview](https://prodocs.easyeda.com/en/pcb/view-preview-3d/))

## OrCAD X

Cadence's reference documentation was not reachable, and two Cadence blog pages returned only their summaries. The points below come from EMA Design Automation's training pages for OrCAD X Presto 23.1 (EMA is a Cadence reseller, so this is not the source that owns the facts) and should be read as unconfirmed.

- **Attachment.** In the footprint editor, the Properties panel has a 3D Model entry; adding a STEP file switches to a 3D view. The mapping is saved into the footprint file (`.dra`).
- **Positioning.** Under 3D Location: Manual (drag in X, Y, Z), Top or Bottom (snap the model to that side of the footprint), a snap that picks a pad and then the matching model pin, and numeric Offset and Rotation. "The mapping of some 3D models may need to be completed manually to achieve the correct position and orientation."
  ([How to Map a 3D Model to Components in OrCAD X Presto](https://www.ema-eda.com/how-to-page/how-to-map-a-3d-model-to-components-in-orcad-x-presto/), updated 2024-07-23)
- **Checks.** The Constraint Manager has a 3D domain with a Component to Component worksheet. A constraint set of usage `Spacing3D` holds horizontal and vertical 3D clearances per component pair, and the check is switched on under Analysis Mode > 3 Dimensional. Violations appear in the DRC list and in the 3D canvas; the page's example is a header colliding with an enclosure, reported as an actual spacing of 0.
  ([How to Find 3D DRC Errors in OrCAD X Presto](https://www.ema-eda.com/how-to-page/how-to-find-3d-drc-errors-in-orcad-x-presto/), updated 2025-01-16)
- **Fallback body.** Cadence community forum replies, which are not documentation, say that without a mapped STEP model the tools use the footprint's place-bound shape with its height property, or a default symbol height. This was not confirmed in any official page.
- **Not established:** accepted formats other than STEP, where model files are searched, behaviour on a missing file, and several models per footprint.

## DipTrace

From the [DipTrace online help](https://diptrace.com/support/tutorials/help_html/), sections 3.13 and 5.3.

- **What a body is.** A pattern (footprint) may have a 3D model attached, in the Pattern Editor or later in PCB Layout. The 3D Model dialog offers three sources:
  1. **A model file** from the model library list or from disk. "DipTrace supports 3DS, VRML, STEP, and IGES files."
  2. **IPC-7351 Model Generator**, "available only if Pattern Generator was used to make a footprint". "In Pattern Generator, footprints are created with 3D models. Those models are generated based on IPC-7351 Recovery Code".
  3. **A model by Component Outline:** a shape placed on the Top Outline layer is extruded to a typed model height. A generator option copies the datasheet's maximum height (`Amax`) into that field. The help says this "suits best for SMD parts".
- **Positioning.** "DipTrace automatically places a 3D model to fit the pattern's drawing"; after that, shift, rotation angle and scale are typed per axis.
- **Finding models.** Models live in folders (by default `models3d` in the DipTrace directory), listed in a search dialog; folders can be given through user path variables. When the 3D preview opens, components without a model are matched to model files "by comparing the component pattern with available 3D models". Found models stay attached only for that preview session unless "Save all new models to project" is ticked.
- **Missing models.** "If a component does not have a 3D model attached, you will see only the component's footprint."
- **Export.** STEP and VRML 2.0, from the 3D preview. "STEP model does not include the copper traces."
- **Height.** Components have a Package Height property that placement reports can list. No height or collision rule was found in the help.
- **Not established:** whether the library file stores the model itself or only its path, and whether a pattern can hold more than one model. The dialog is described in the singular throughout.

## Horizon EDA

The documentation says only that packages are extended "into the 3rd dimension by adding a 3D model in industry-standard STEP format".
([feature overview](https://github.com/horizon-eda/horizon-docs/blob/master/feature-overview.rst))
The rest is from source at commit `79e3422` (master, 2026-11-17).

- **What a body is.** A package has `models`, a map from UUID to model, and a `default_model`. Each model stores a filename, shifts `x`, `y`, `z`, rotations `roll`, `pitch`, `yaw`, and two heights, `height_top` and `height_bot`. An older single `model_filename` field is still read.
  ([package.hpp](https://github.com/horizon-eda/horizon/blob/master/src/pool/package.hpp), [package.cpp](https://github.com/horizon-eda/horizon/blob/master/src/pool/package.cpp))
- **Formats and storage.** The file chooser accepts only `*.step` and `*.stp`, and rejects files outside the pool: "Model has to be in the pool directory". The stored filename is relative to the pool base path. Nothing is embedded in the package file.
  ([imp_package_3d.cpp](https://github.com/horizon-eda/horizon/blob/master/src/imp/3d/imp_package_3d.cpp))
- **Several per package.** Yes, but as alternatives and not as parts of one body: a placed package on a board has a single `model` UUID that selects one of them.
  ([board_package.hpp](https://github.com/horizon-eda/horizon/blob/master/src/board/board_package.hpp))
- **Positioning.** In the package editor's 3D window: numeric shift and rotation, a Place button, an Align tool and a Project tool (bottom-up or top-down). The changelog lists "Add 3D model alignment", "Add 3D model projection" and "Add button for centering 3D models".
  ([model_editor.cpp](https://github.com/horizon-eda/horizon/blob/master/src/imp/3d/model_editor.cpp), [CHANGELOG.md](https://github.com/horizon-eda/horizon/blob/master/CHANGELOG.md))
- **Heights and the height rule.** The top and bottom heights are stored on the model, and a button fills them "from model". Boards have Height Restriction objects: a polygon on a package layer with a maximum height. The check intersects each restriction with the package outlines in 2D and compares the selected model's stored height, allowing for board thickness on a flipped part. It warns when a package "has no 3D model" or has no height, and fails with "height (…) exceeds height restriction of …". The check never reads the STEP geometry itself.
  ([board_rules_check_height_restrictions.cpp](https://github.com/horizon-eda/horizon/blob/master/src/board/board_rules_check_height_restrictions.cpp))
- **Export.** STEP export exists; the changelog notes it omits non-populated models and copies STEP models into the project pool when needed.
- **Not established:** what the 3D view draws when a model file is missing, and any model generator. No generator was seen in the files read.

## Where the tools differ

- **Simple shapes versus files.** Altium has stored primitive bodies (extruded polygon, cylinder, sphere) as well as models. KiCad's releases have only file references; its development branch adds a parametric extruded body that takes its outline from an existing layer or the pad bounding box and stores no shape of its own. DipTrace extrudes an outline shape to a height. EasyEDA Pro has no stored simple body but builds a box at export time. Horizon has only STEP files. Fusion's body is always a full solid model.
- **Where the model lives.** Embedded in the library file (Altium by default; KiCad optionally since 9.0), a path with variables (KiCad by default), a path relative to a library root (Horizon), a library record referenced by UUID (EasyEDA Pro, Altium Workspace models), a package object in the library (Fusion), or model folders searched by name (DipTrace).
- **How placement is expressed.** Every tool has an offset and a rotation. They differ in the rest:
  - **Scale:** KiCad and DipTrace store a scale factor per axis; EasyEDA Pro stores a target size and normalises the model to its bounding box first; Altium, Horizon and Fusion have no scale.
  - **Standoff:** Altium and KiCad master name it as its own field; elsewhere it is the Z offset.
  - **Geometric aids:** Altium (snap points, align face with board), Fusion (face on plane, centre between edges, point to point), OrCAD X (snap to side, pin to pad) and Horizon (align, project) pick geometry on the model. KiCad's releases and DipTrace use typed numbers with a preview; EasyEDA Pro and DipTrace start from an automatic fit.
  - **Library policy:** KiCad's official library forbids any offset, rotation or scale in the footprint, so models are aligned in the modelling tool.
- **Several bodies.** Altium composes a part from several bodies (case plus pins). KiCad lists several models, all drawn. Horizon lists several models and a board package picks one. EasyEDA Pro has one. KiCad master currently allows one extruded body.
- **Missing files.** KiCad treats a dangling path as normal (its library conventions require the reference even with no file), draws nothing, and reports the file at export. EasyEDA Pro lists failures at export and substitutes a box when no model is bound. Altium reports a load error for a broken Workspace model. DipTrace shows the bare footprint and tries a search by name.
- **Height.** Altium, EasyEDA Pro, DipTrace and KiCad master keep a height on the footprint or component apart from any model. Horizon keeps top and bottom heights on each model, filled from the geometry on request.
- **Checks.** Altium checks height (Min/Preferred/Max) and true 3D clearance between bodies, with the bounding box and Height as fallback. OrCAD X, per the reseller source, has horizontal and vertical 3D clearances per component pair. Horizon checks a stored height against height-restriction areas in 2D. KiCad's releases check only 2D Courtyards. No height or collision check was found for EasyEDA Pro or DipTrace.
- **Generation for standard packages.** Altium's IPC wizard and Fusion's Package Generator produce footprint and model from one set of package dimensions inside the tool. DipTrace's Pattern Generator does the same from IPC-7351 data. KiCad generates its library's models offline with CadQuery scripts that now sit in the same repository as the footprint generators. No generator was found for Horizon or EasyEDA Pro.
- **Export.** A mesh format cannot feed a B-rep export: KiCad cannot put VRML models into STEP and substitutes a STEP file of the same name; EasyEDA Pro's STEP export needs a STEP-format model. Altium lets the user choose between simple bodies and models when a component has both. KiCad master exports its extruded bodies to STEP.

## Not verified

- **OrCAD X:** everything in that section; Cadence's own documentation was not reached.
- **Altium:** what is drawn when a folder-linked model file is missing; whether the STEP export page (version 21) still matches the current release. The IPC wizard's batch generator was not read for 3D options.
- **KiCad:** whether current library footprints reference `.step` (library conventions) or `.wrl` (9.0 documentation); how "Auto" picks the outline layer for the unreleased extruded body beyond the placeholder order quoted above.
- **Fusion and Eagle:** several packages per footprint, missing-package behaviour, checks in the board editor; Eagle was not researched on its own. An Autodesk blog post on library 3D mapping returned HTTP 403.
- **EasyEDA Pro:** the names of the calibration fields in the 3D Model Manager (taken here from the file-format page), and the unit of the transform values.
- **DipTrace:** whether a library stores model data or a path, and several models per pattern. The Pattern Editor PDF help could not be parsed; the HTML help was used.
- **Horizon EDA:** behaviour when a model file is missing.
